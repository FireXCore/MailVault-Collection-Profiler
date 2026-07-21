use std::time::{Duration, Instant};

use profiler_core::{
    AvailabilityState, FileStatCheckpoint, FileStatObservation, FileStatRequest, FileStatResult,
    FileStatStore, PhysicalObjectResolver, ProfilerError, ProfilerResult, ProgressEvent,
    ProgressSink, ProgressUnit, RunStage, SizeState, StageState,
};
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};

#[allow(clippy::too_many_lines)]
pub(crate) fn run_file_stat(
    request: &FileStatRequest,
    store: &mut dyn FileStatStore,
    resolver: &dyn PhysicalObjectResolver,
    progress: &dyn ProgressSink,
) -> ProfilerResult<FileStatResult> {
    validate_options(request)?;
    let workers = resolve_workers(request.options.workers);
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(|index| format!("mailvault-file-stat-{index}"))
        .build()
        .map_err(|error| {
            ProfilerError::Internal(format!("creating file-stat worker pool: {error}"))
        })?;

    let (total_objects, total_bytes) = store.count_file_stat_objects(&request.collection_id)?;
    let mut checkpoint = store
        .load_file_stat_checkpoint(&request.run_id)?
        .unwrap_or_else(FileStatCheckpoint::empty);
    validate_checkpoint(&checkpoint, total_objects, total_bytes)?;

    let initial_completed_objects = checkpoint.completed_objects;
    let initial_completed_bytes = checkpoint.completed_bytes;
    let started = Instant::now();
    progress.send(progress_event(
        request,
        &checkpoint,
        total_objects,
        total_bytes,
        workers,
        started,
        initial_completed_objects,
        initial_completed_bytes,
        StageState::Running,
        None,
    ))?;

    loop {
        let batch = store.load_file_stat_batch(
            &request.collection_id,
            checkpoint.last_sha256.as_deref(),
            request.options.batch_size,
        )?;
        if batch.is_empty() {
            if checkpoint.completed_objects != total_objects
                || checkpoint.completed_bytes != total_bytes
            {
                return Err(ProfilerError::Internal(format!(
                    "file-stat ended at {}/{} objects and {}/{} bytes",
                    checkpoint.completed_objects,
                    total_objects,
                    checkpoint.completed_bytes,
                    total_bytes
                )));
            }
            break;
        }

        let observations = inspect_batch(&pool, request, resolver, &batch);
        let batch_objects = u64::try_from(batch.len())
            .map_err(|_| ProfilerError::Internal("file-stat batch length overflowed u64".into()))?;
        let batch_bytes = batch.iter().try_fold(0_u64, |total, item| {
            total.checked_add(item.expected_size_bytes).ok_or_else(|| {
                ProfilerError::Internal("file-stat expected-byte counter overflowed u64".into())
            })
        })?;
        checkpoint.completed_objects = checkpoint
            .completed_objects
            .checked_add(batch_objects)
            .ok_or_else(|| {
                ProfilerError::Internal("file-stat object counter overflowed u64".into())
            })?;
        checkpoint.completed_bytes = checkpoint
            .completed_bytes
            .checked_add(batch_bytes)
            .ok_or_else(|| {
                ProfilerError::Internal("file-stat byte counter overflowed u64".into())
            })?;
        let (batch_warnings, batch_errors) = observation_issue_counts(&observations);
        checkpoint.warnings = checkpoint.warnings.saturating_add(batch_warnings);
        checkpoint.errors = checkpoint.errors.saturating_add(batch_errors);
        checkpoint.sequence = checkpoint.sequence.saturating_add(1);
        checkpoint.last_sha256 = batch.last().map(|item| item.sha256.clone());

        store.commit_file_stat_batch(request, &observations, &checkpoint)?;
        let current = batch.last().map(|item| {
            format!(
                "{}… · {}",
                item.sha256.get(..12).unwrap_or(&item.sha256),
                item.source_locator
            )
        });
        progress.send(progress_event(
            request,
            &checkpoint,
            total_objects,
            total_bytes,
            workers,
            started,
            initial_completed_objects,
            initial_completed_bytes,
            StageState::Running,
            current,
        ))?;
    }

    let summary = store.finalize_file_stat(request)?;
    checkpoint.sequence = checkpoint.sequence.saturating_add(1);
    progress.send(progress_event(
        request,
        &checkpoint,
        total_objects,
        total_bytes,
        0,
        started,
        initial_completed_objects,
        initial_completed_bytes,
        StageState::Completed,
        None,
    ))?;

    Ok(FileStatResult {
        run_id: request.run_id.clone(),
        collection_id: request.collection_id.clone(),
        summary,
    })
}

fn inspect_batch(
    pool: &ThreadPool,
    request: &FileStatRequest,
    resolver: &dyn PhysicalObjectResolver,
    batch: &[profiler_core::FileStatWorkItem],
) -> Vec<FileStatObservation> {
    pool.install(|| {
        batch
            .par_iter()
            .map(|item| resolver.inspect(&request.archive_root, item))
            .collect()
    })
}

fn validate_options(request: &FileStatRequest) -> ProfilerResult<()> {
    if request.options.batch_size == 0 {
        return Err(ProfilerError::InvalidArgument(
            "file-stat batch size must be greater than zero".into(),
        ));
    }
    if request.options.batch_size > 100_000 {
        return Err(ProfilerError::InvalidArgument(
            "file-stat batch size must not exceed 100000".into(),
        ));
    }
    if request.options.workers > 64 {
        return Err(ProfilerError::InvalidArgument(
            "file-stat worker count must not exceed 64".into(),
        ));
    }
    Ok(())
}

fn resolve_workers(configured: u32) -> usize {
    if configured > 0 {
        return usize::try_from(configured).unwrap_or(1);
    }
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, 4)
}

fn validate_checkpoint(
    checkpoint: &FileStatCheckpoint,
    total_objects: u64,
    total_bytes: u64,
) -> ProfilerResult<()> {
    if checkpoint.completed_objects > total_objects || checkpoint.completed_bytes > total_bytes {
        return Err(ProfilerError::Internal(format!(
            "file-stat checkpoint exceeds stage totals: {}/{} objects, {}/{} bytes",
            checkpoint.completed_objects, total_objects, checkpoint.completed_bytes, total_bytes
        )));
    }
    if checkpoint.completed_objects == 0 && checkpoint.last_sha256.is_some() {
        return Err(ProfilerError::Internal(
            "file-stat checkpoint has a cursor but zero completed objects".into(),
        ));
    }
    Ok(())
}

fn observation_issue_counts(observations: &[FileStatObservation]) -> (u64, u64) {
    observations
        .iter()
        .fold(
            (0, 0),
            |(warnings, errors), observation| match observation.availability_state {
                AvailabilityState::Available if observation.size_state == SizeState::Match => {
                    (warnings, errors)
                }
                AvailabilityState::Missing => (warnings.saturating_add(1), errors),
                _ => (warnings, errors.saturating_add(1)),
            },
        )
}

#[allow(clippy::too_many_arguments)]
fn progress_event(
    request: &FileStatRequest,
    checkpoint: &FileStatCheckpoint,
    total_objects: u64,
    total_bytes: u64,
    workers: usize,
    started: Instant,
    initial_completed_objects: u64,
    initial_completed_bytes: u64,
    state: StageState,
    current_object_display: Option<String>,
) -> ProgressEvent {
    let elapsed = started.elapsed();
    let session_bytes = checkpoint
        .completed_bytes
        .saturating_sub(initial_completed_bytes);
    let session_objects = checkpoint
        .completed_objects
        .saturating_sub(initial_completed_objects);
    let throughput = throughput_per_second(session_bytes, session_objects, elapsed);
    let remaining_work = if total_bytes > 0 {
        total_bytes.saturating_sub(checkpoint.completed_bytes)
    } else {
        total_objects.saturating_sub(checkpoint.completed_objects)
    };

    ProgressEvent {
        run_id: request.run_id.clone(),
        sequence: checkpoint.sequence,
        stage: RunStage::FileStat,
        stage_state: state,
        unit: ProgressUnit::Objects,
        completed_items: checkpoint.completed_objects,
        total_items: Some(total_objects),
        completed_bytes: checkpoint.completed_bytes,
        total_bytes: Some(total_bytes),
        elapsed_ms: duration_ms(elapsed),
        instant_throughput: None,
        smoothed_throughput: throughput,
        eta_ms: if state == StageState::Completed {
            Some(0)
        } else {
            estimate_eta(remaining_work, throughput)
        },
        active_workers: u32::try_from(workers).unwrap_or(u32::MAX),
        queue_depth: 0,
        warnings: checkpoint.warnings,
        errors: checkpoint.errors,
        current_object_display,
        checkpoint_sequence: checkpoint.sequence,
    }
}

#[allow(clippy::cast_precision_loss)]
fn throughput_per_second(bytes: u64, objects: u64, elapsed: Duration) -> Option<f64> {
    let work = if bytes > 0 { bytes } else { objects };
    let seconds = elapsed.as_secs_f64();
    (work > 0 && seconds >= 0.25).then_some(work as f64 / seconds)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn estimate_eta(remaining_work: u64, throughput: Option<f64>) -> Option<u64> {
    let rate = throughput?;
    if rate <= 0.0 || !rate.is_finite() {
        return None;
    }
    let milliseconds = (remaining_work as f64 / rate) * 1_000.0;
    (milliseconds.is_finite() && milliseconds >= 0.0).then_some(milliseconds as u64)
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use fs2::FileExt;

use profiler_core::{
    FormatIdentificationRequest, FormatIdentificationResult, FormatObservation, FormatPage,
    FormatPageRequest, FormatRunRegistration, FormatRunStartRequest, FormatSummary,
    FormatToolIdentity, FormatWorkItem, ProfilerError, ProfilerResult, ProgressEvent, ProgressSink,
    ProgressUnit, RunStage, StageState,
};
use profiler_format_siegfried::{
    BatchWorkspace, ResolvedFormatInput, SiegfriedOptions, SiegfriedRunner, skipped_observation,
};
use profiler_storage_sqlite::ProfilerStore;
use sha2::{Digest, Sha256};

#[derive(Debug, Default, Clone, Copy)]
pub struct ExactFormatEngine;

impl ExactFormatEngine {
    pub fn probe_tool(
        &self,
        executable: Option<PathBuf>,
        signature: Option<PathBuf>,
        workers: u32,
    ) -> ProfilerResult<FormatToolIdentity> {
        let options = SiegfriedOptions {
            executable,
            signature,
            workers: resolve_worker_count(workers),
            timeout: Duration::from_secs(30),
        };
        let runner = SiegfriedRunner::probe(&options)?;
        Ok(runner.identity().clone())
    }

    pub fn identify(
        &self,
        request: &FormatIdentificationRequest,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<FormatIdentificationResult> {
        validate_request(request)?;
        let _format_lock = FormatRunLock::acquire(&request.workspace_root)?;
        let mut prepared = prepare_format_run(request)?;
        let started = Instant::now();

        let execution = execute_format_run(
            request,
            progress,
            &prepared.runner,
            &mut prepared.store,
            &prepared.registration,
            prepared.workers,
            started,
        );
        if let Err(error) = &execution {
            let _ = prepared
                .store
                .fail_format_run(&prepared.registration.format_run_id, error);
        }

        let completed = execution?;
        emit_completed_progress(
            progress,
            &prepared.registration.format_run_id,
            completed.sequence,
            &completed.summary,
            started,
        )?;

        Ok(FormatIdentificationResult {
            format_run_id: prepared.registration.format_run_id,
            baseline_run_id: request.baseline_run_id.clone(),
            configuration_fingerprint: prepared.configuration_fingerprint,
            summary: completed.summary,
        })
    }

    pub fn summary(
        &self,
        profiler_database: &Path,
        baseline_run_id: &str,
    ) -> ProfilerResult<FormatSummary> {
        ProfilerStore::open_read_only(profiler_database)?.format_summary(baseline_run_id)
    }

    pub fn page(
        &self,
        profiler_database: &Path,
        request: &FormatPageRequest,
    ) -> ProfilerResult<FormatPage> {
        ProfilerStore::open_read_only(profiler_database)?.format_page(request)
    }
}

struct PreparedFormatRun {
    workers: u32,
    runner: SiegfriedRunner,
    store: ProfilerStore,
    registration: FormatRunRegistration,
    configuration_fingerprint: String,
}

struct CompletedFormatRun {
    summary: FormatSummary,
    sequence: u64,
}

struct FormatProgressState {
    started: Instant,
    last_progress_time: Instant,
    last_completed: u64,
    workers: u32,
}

struct ProcessedFormatBatch {
    checkpoint: String,
    summary: FormatSummary,
}

fn prepare_format_run(request: &FormatIdentificationRequest) -> ProfilerResult<PreparedFormatRun> {
    let workers = resolve_worker_count(request.options.workers);
    let runner_options = SiegfriedOptions {
        executable: request.siegfried_path.clone(),
        signature: request.signature_path.clone(),
        workers,
        timeout: Duration::from_secs(request.options.timeout_seconds),
    };
    let runner = SiegfriedRunner::probe(&runner_options)?;
    let configuration_fingerprint = configuration_fingerprint(request, runner.identity(), workers)?;
    let mut store = ProfilerStore::open_existing(&request.profiler_database)?;
    let registration = store.begin_format_run(&FormatRunStartRequest {
        baseline_run_id: request.baseline_run_id.as_str(),
        tool: runner.identity(),
        configuration_fingerprint: configuration_fingerprint.as_str(),
        batch_size: request.options.batch_size,
        worker_count: workers,
        timeout_seconds: request.options.timeout_seconds,
        resume: request.options.resume,
    })?;

    Ok(PreparedFormatRun {
        workers,
        runner,
        store,
        registration,
        configuration_fingerprint,
    })
}

fn execute_format_run(
    request: &FormatIdentificationRequest,
    progress: &dyn ProgressSink,
    runner: &SiegfriedRunner,
    store: &mut ProfilerStore,
    registration: &FormatRunRegistration,
    workers: u32,
    started: Instant,
) -> ProfilerResult<CompletedFormatRun> {
    let mut after_sha256 = registration.resume_after_sha256.clone();
    let mut sequence = registration.checkpoint_sequence;
    let mut progress_state = FormatProgressState {
        started,
        last_progress_time: Instant::now(),
        last_completed: registration.completed_objects,
        workers,
    };

    loop {
        let work = store.load_format_work_batch(
            &registration.collection_id,
            after_sha256.as_deref(),
            request.options.batch_size,
        )?;
        if work.is_empty() {
            break;
        }

        sequence = sequence.saturating_add(1);
        let processed =
            process_format_batch(request, runner, store, registration, &work, sequence)?;
        emit_progress(
            progress,
            &registration.format_run_id,
            sequence,
            &processed.summary,
            &mut progress_state,
        )?;
        after_sha256 = Some(processed.checkpoint);

        if work.len() < request.options.batch_size as usize {
            break;
        }
    }

    let summary =
        store.complete_format_run(&registration.format_run_id, &request.baseline_run_id)?;
    Ok(CompletedFormatRun { summary, sequence })
}

fn process_format_batch(
    request: &FormatIdentificationRequest,
    runner: &SiegfriedRunner,
    store: &mut ProfilerStore,
    registration: &FormatRunRegistration,
    work: &[FormatWorkItem],
    sequence: u64,
) -> ProfilerResult<ProcessedFormatBatch> {
    let checkpoint = work
        .last()
        .map(|item| item.sha256.clone())
        .ok_or_else(|| ProfilerError::Internal("format batch was unexpectedly empty".into()))?;
    let (mut observations, eligible) = resolve_batch_inputs(request, work);

    if !eligible.is_empty() {
        let staging = BatchWorkspace::new(
            &request.workspace_root,
            &registration.format_run_id,
            sequence,
        )?;
        observations.extend(runner.identify_batch(&eligible, &staging)?);
    }

    observations.sort_by(|left, right| left.sha256.cmp(&right.sha256));
    let summary = store.commit_format_observations(
        &registration.format_run_id,
        &request.baseline_run_id,
        &observations,
        &checkpoint,
        sequence,
    )?;

    Ok(ProcessedFormatBatch {
        checkpoint,
        summary,
    })
}

fn resolve_batch_inputs(
    request: &FormatIdentificationRequest,
    work: &[FormatWorkItem],
) -> (Vec<FormatObservation>, Vec<ResolvedFormatInput>) {
    let mut observations = Vec::with_capacity(work.len());
    let mut eligible = Vec::new();

    for item in work {
        if item.expected_size_bytes == 0 || item.availability_state != "available" {
            observations.push(skipped_observation(item));
            continue;
        }

        match resolve_object_path(&request.archive_root, &item.canonical_path_display) {
            Ok(source_path) => eligible.push(ResolvedFormatInput {
                item: item.clone(),
                source_path,
            }),
            Err(error) => observations.push(path_resolution_error(item, &error)),
        }
    }

    (observations, eligible)
}

fn path_resolution_error(item: &FormatWorkItem, error: &ProfilerError) -> FormatObservation {
    FormatObservation {
        content_object_id: item.content_object_id.clone(),
        sha256: item.sha256.clone(),
        state: profiler_core::FormatState::ToolError,
        source_mime_type: item.source_mime_type.clone(),
        preferred_extension: item.preferred_extension.clone(),
        staging_mode: "path_resolution_failed".into(),
        primary_identifier: None,
        primary_format_name: None,
        primary_format_version: None,
        primary_mime_type: None,
        match_count: 0,
        extension_checked: false,
        extension_mismatch: false,
        error_code: Some("SOURCE_PATH_UNAVAILABLE".into()),
        error_message: Some(error.to_string()),
        matches: Vec::new(),
        observed_at: now_text(),
    }
}

#[derive(Debug)]
struct FormatRunLock {
    file: File,
}

impl FormatRunLock {
    fn acquire(workspace_root: &Path) -> ProfilerResult<Self> {
        fs::create_dir_all(workspace_root).map_err(|source| ProfilerError::Io {
            operation: "creating exact-format workspace root",
            path: workspace_root.to_path_buf(),
            source,
        })?;
        let path = workspace_root.join(".mailvault-profiler.format.lock");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| ProfilerError::Io {
                operation: "opening exact-format lock",
                path: path.clone(),
                source,
            })?;
        file.try_lock_exclusive().map_err(|source| {
            if source.kind() == fs2::lock_contended_error().kind() {
                ProfilerError::contract(
                    profiler_core::ErrorCode::FormatRunAlreadyActive,
                    "another exact-format identification process holds the workspace lock",
                    true,
                )
            } else {
                ProfilerError::Io {
                    operation: "acquiring exact-format lock",
                    path: path.clone(),
                    source,
                }
            }
        })?;
        file.set_len(0).map_err(|source| ProfilerError::Io {
            operation: "resetting exact-format lock metadata",
            path: path.clone(),
            source,
        })?;
        file.seek(SeekFrom::Start(0))
            .map_err(|source| ProfilerError::Io {
                operation: "seeking exact-format lock metadata",
                path: path.clone(),
                source,
            })?;
        let metadata = serde_json::to_vec(&serde_json::json!({
            "pid": std::process::id(),
            "startedAt": now_text(),
        }))
        .map_err(|error| {
            ProfilerError::Internal(format!("serializing exact-format lock: {error}"))
        })?;
        file.write_all(&metadata)
            .map_err(|source| ProfilerError::Io {
                operation: "writing exact-format lock metadata",
                path: path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| ProfilerError::Io {
            operation: "syncing exact-format lock metadata",
            path,
            source,
        })?;
        Ok(Self { file })
    }
}

impl Drop for FormatRunLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn validate_request(request: &FormatIdentificationRequest) -> ProfilerResult<()> {
    if request.baseline_run_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument(
            "baseline run id cannot be empty".into(),
        ));
    }
    if request.options.batch_size == 0 || request.options.batch_size > 10_000 {
        return Err(ProfilerError::InvalidArgument(
            "format batch size must be between 1 and 10000".into(),
        ));
    }
    if request.options.timeout_seconds < 30 {
        return Err(ProfilerError::InvalidArgument(
            "format timeout must be at least 30 seconds".into(),
        ));
    }
    if !request.profiler_database.is_file() {
        return Err(ProfilerError::InvalidPath {
            message: "profiler database does not exist".into(),
            path: request.profiler_database.clone(),
        });
    }
    if !request.archive_root.is_dir() {
        return Err(ProfilerError::InvalidPath {
            message: "MailVault archive root does not exist".into(),
            path: request.archive_root.clone(),
        });
    }
    Ok(())
}

fn resolve_worker_count(configured: u32) -> u32 {
    if configured > 0 {
        return configured.clamp(1, 16);
    }
    std::thread::available_parallelism()
        .map_or(2, |value| u32::try_from(value.get()).unwrap_or(4))
        .clamp(1, 4)
}

fn resolve_object_path(archive_root: &Path, locator: &str) -> ProfilerResult<PathBuf> {
    if locator.contains('\0') {
        return Err(ProfilerError::InvalidPath {
            message: "content locator contains NUL".into(),
            path: PathBuf::from(locator),
        });
    }
    let root = fs::canonicalize(archive_root).map_err(|source| ProfilerError::Io {
        operation: "canonicalizing MailVault root for exact-format identification",
        path: archive_root.to_path_buf(),
        source,
    })?;
    let candidate = root.join(locator.replace('/', std::path::MAIN_SEPARATOR_STR));
    let resolved = fs::canonicalize(&candidate).map_err(|source| ProfilerError::Io {
        operation: "resolving MailVault content object for exact-format identification",
        path: candidate,
        source,
    })?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err(ProfilerError::InvalidPath {
            message: "content object escapes archive root or is not a regular file".into(),
            path: resolved,
        });
    }
    Ok(resolved)
}

fn configuration_fingerprint(
    request: &FormatIdentificationRequest,
    tool: &FormatToolIdentity,
    workers: u32,
) -> ProfilerResult<String> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "contract": "mailvault-profiler-exact-format-v1",
        "toolName": tool.tool_name,
        "toolVersion": tool.tool_version,
        "executableSha256": tool.executable_sha256,
        "signatureVersion": tool.signature_version,
        "signatureSha256": tool.signature_sha256,
        "extensionEvidence": "symlink-alias-or-unchecked",
        "batchSize": request.options.batch_size,
        "workers": workers,
        "timeoutSeconds": request.options.timeout_seconds,
        "containerExpansion": false,
    }))
    .map_err(|error| {
        ProfilerError::Internal(format!("serializing exact-format configuration: {error}"))
    })?;
    let mut digest = Sha256::new();
    digest.update(b"mailvault-profiler-exact-format-v1\0");
    digest.update(payload);
    Ok(hex::encode(digest.finalize()))
}

fn emit_progress(
    progress: &dyn ProgressSink,
    format_run_id: &str,
    sequence: u64,
    summary: &FormatSummary,
    state: &mut FormatProgressState,
) -> ProfilerResult<()> {
    let elapsed_since_last = state.last_progress_time.elapsed();
    let completed_since_last = summary
        .completed_objects
        .saturating_sub(state.last_completed);
    let instant = throughput_per_second(completed_since_last, elapsed_since_last);
    let smoothed = throughput_per_second(summary.completed_objects, state.started.elapsed());
    let eta_ms = estimate_eta_ms(
        state.started.elapsed(),
        summary.completed_objects,
        summary.total_objects,
    );

    progress.send(ProgressEvent {
        run_id: format_run_id.into(),
        sequence,
        stage: RunStage::FormatIdentification,
        stage_state: StageState::Running,
        unit: ProgressUnit::Objects,
        completed_items: summary.completed_objects,
        total_items: Some(summary.total_objects),
        completed_bytes: summary.completed_bytes,
        total_bytes: Some(summary.total_bytes),
        elapsed_ms: elapsed_ms(state.started),
        instant_throughput: instant,
        smoothed_throughput: smoothed,
        eta_ms,
        active_workers: state.workers,
        queue_depth: 0,
        warnings: summary.unknown + summary.ambiguous + summary.extension_mismatches,
        errors: summary.tool_errors,
        current_object_display: None,
        checkpoint_sequence: sequence,
    })?;
    state.last_progress_time = Instant::now();
    state.last_completed = summary.completed_objects;
    Ok(())
}

fn emit_completed_progress(
    progress: &dyn ProgressSink,
    format_run_id: &str,
    sequence: u64,
    summary: &FormatSummary,
    started: Instant,
) -> ProfilerResult<()> {
    progress.send(ProgressEvent {
        run_id: format_run_id.into(),
        sequence: sequence.saturating_add(1),
        stage: RunStage::FormatIdentification,
        stage_state: StageState::Completed,
        unit: ProgressUnit::Objects,
        completed_items: summary.completed_objects,
        total_items: Some(summary.total_objects),
        completed_bytes: summary.completed_bytes,
        total_bytes: Some(summary.total_bytes),
        elapsed_ms: elapsed_ms(started),
        instant_throughput: None,
        smoothed_throughput: None,
        eta_ms: Some(0),
        active_workers: 0,
        queue_depth: 0,
        warnings: summary.unknown + summary.ambiguous + summary.extension_mismatches,
        errors: summary.tool_errors,
        current_object_display: None,
        checkpoint_sequence: sequence.saturating_add(1),
    })
}

fn throughput_per_second(completed: u64, elapsed: Duration) -> Option<f64> {
    const SCALE: u128 = 1_000;

    let elapsed_ms = elapsed.as_millis();
    if completed == 0 || elapsed_ms == 0 {
        return None;
    }

    let scaled_rate = u128::from(completed)
        .saturating_mul(1_000)
        .saturating_mul(SCALE)
        / elapsed_ms;
    let bounded_rate = u32::try_from(scaled_rate).unwrap_or(u32::MAX);
    Some(f64::from(bounded_rate) / 1_000.0)
}

fn estimate_eta_ms(elapsed: Duration, completed: u64, total: u64) -> Option<u64> {
    if completed < 3 {
        return None;
    }

    let remaining = total.saturating_sub(completed);
    if remaining == 0 {
        return Some(0);
    }

    let elapsed_ms = elapsed.as_millis();
    if elapsed_ms == 0 {
        return None;
    }

    let completed_wide = u128::from(completed);
    let numerator = elapsed_ms.saturating_mul(u128::from(remaining));
    let rounded = numerator.saturating_add(completed_wide / 2) / completed_wide;
    Some(u64::try_from(rounded).unwrap_or(u64::MAX))
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn now_text() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting is infallible for OffsetDateTime")
}

#[cfg(test)]
mod tests {
    use super::{estimate_eta_ms, throughput_per_second};
    use std::time::Duration;

    #[test]
    fn throughput_uses_integer_scaling_without_lossy_casts() {
        assert_eq!(throughput_per_second(10, Duration::from_secs(2)), Some(5.0));
    }

    #[test]
    fn eta_uses_saturating_integer_arithmetic() {
        assert_eq!(estimate_eta_ms(Duration::from_secs(2), 4, 10), Some(3_000));
    }

    #[test]
    fn eta_waits_for_a_minimum_sample() {
        assert_eq!(estimate_eta_ms(Duration::from_secs(2), 2, 10), None);
    }
}

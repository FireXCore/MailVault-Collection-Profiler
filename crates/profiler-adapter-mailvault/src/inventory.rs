use std::time::Instant;

use profiler_core::{
    InventoryBatch, InventoryCheckpoint, InventoryRequest, InventoryResult, InventorySink,
    InventoryTable, ProfilerError, ProfilerResult, ProgressEvent, ProgressSink, ProgressUnit,
    RunStage, SourceBlobRecord, SourceMessageRecord, SourceOccurrenceRecord, SourcePartRecord,
    SourceParticipantRecord, SourceRelationRecord, StageState,
};
use rusqlite::{Connection, Row, params};

use crate::preflight::{open_read_only, read_metrics};

#[allow(clippy::too_many_lines)]
pub(crate) fn run_inventory(
    request: &InventoryRequest,
    sink: &mut dyn InventorySink,
    progress: &dyn ProgressSink,
) -> ProfilerResult<InventoryResult> {
    validate_request(request)?;
    let connection = open_read_only(&request.snapshot_database)?;
    let actual_metrics = read_metrics(&connection)?;
    if actual_metrics != request.expected_metrics {
        return Err(ProfilerError::IncompatibleSource(
            "source snapshot metrics differ from the signed snapshot manifest".into(),
        ));
    }

    let table_totals = [
        (InventoryTable::Messages, actual_metrics.messages),
        (
            InventoryTable::MessageOccurrences,
            actual_metrics.message_occurrences,
        ),
        (InventoryTable::Participants, actual_metrics.participants),
        (InventoryTable::Blobs, actual_metrics.blobs),
        (InventoryTable::Parts, actual_metrics.mime_parts),
        (
            InventoryTable::MessageRelations,
            actual_metrics.message_relations,
        ),
    ];
    let total_rows = table_totals
        .iter()
        .map(|(_, rows)| *rows)
        .try_fold(0_u64, u64::checked_add)
        .ok_or_else(|| ProfilerError::Internal("inventory row total overflowed u64".into()))?;

    let checkpoints = table_totals
        .iter()
        .map(|(table, total)| {
            let checkpoint = sink
                .load_checkpoint(&request.run_id, *table)?
                .unwrap_or_else(|| InventoryCheckpoint::empty(*table));
            if checkpoint.completed_rows > *total {
                return Err(ProfilerError::IncompatibleSource(format!(
                    "checkpoint for {table} exceeds snapshot row count"
                )));
            }
            Ok((*table, checkpoint))
        })
        .collect::<ProfilerResult<Vec<_>>>()?;

    let mut completed_rows = checkpoints
        .iter()
        .try_fold(0_u64, |total, (_, checkpoint)| {
            total.checked_add(checkpoint.completed_rows).ok_or_else(|| {
                ProfilerError::Internal("checkpoint row total overflowed u64".into())
            })
        })?;
    let mut sequence = checkpoints
        .iter()
        .map(|(_, checkpoint)| checkpoint.sequence)
        .max()
        .unwrap_or_default();
    let started = Instant::now();

    progress.send(ProgressEvent {
        run_id: request.run_id.clone(),
        sequence,
        stage: RunStage::MetadataInventory,
        stage_state: StageState::Running,
        unit: ProgressUnit::Rows,
        completed_items: completed_rows,
        total_items: Some(total_rows),
        completed_bytes: 0,
        total_bytes: None,
        elapsed_ms: 0,
        instant_throughput: None,
        smoothed_throughput: None,
        eta_ms: None,
        active_workers: 1,
        queue_depth: 0,
        warnings: 0,
        errors: 0,
        current_object_display: None,
        checkpoint_sequence: sequence,
    })?;

    for (table, table_total) in table_totals {
        let checkpoint = checkpoints
            .iter()
            .find_map(|(candidate, checkpoint)| (*candidate == table).then_some(checkpoint.clone()))
            .ok_or_else(|| ProfilerError::Internal(format!("checkpoint missing for {table}")))?;
        let table_completed_before = checkpoint.completed_rows;
        let completed = inventory_table(
            &connection,
            request,
            sink,
            progress,
            table,
            table_total,
            checkpoint,
            &mut sequence,
            completed_rows,
            total_rows,
            started,
        )?;
        completed_rows = completed_rows
            .checked_add(completed.saturating_sub(table_completed_before))
            .ok_or_else(|| ProfilerError::Internal("inventory progress overflowed u64".into()))?;
    }

    let summary = sink.finalize_inventory(request)?;
    sequence = sequence.saturating_add(1);
    progress.send(ProgressEvent {
        run_id: request.run_id.clone(),
        sequence,
        stage: RunStage::MetadataInventory,
        stage_state: StageState::Completed,
        unit: ProgressUnit::Rows,
        completed_items: total_rows,
        total_items: Some(total_rows),
        completed_bytes: 0,
        total_bytes: None,
        elapsed_ms: elapsed_ms(started),
        instant_throughput: None,
        smoothed_throughput: Some(rows_per_second(total_rows, started)),
        eta_ms: Some(0),
        active_workers: 0,
        queue_depth: 0,
        warnings: 0,
        errors: 0,
        current_object_display: None,
        checkpoint_sequence: sequence,
    })?;

    Ok(InventoryResult {
        run_id: request.run_id.clone(),
        source_snapshot_id: request.source_snapshot_id.clone(),
        summary,
    })
}

#[allow(clippy::too_many_arguments)]
fn inventory_table(
    connection: &Connection,
    request: &InventoryRequest,
    sink: &mut dyn InventorySink,
    progress: &dyn ProgressSink,
    table: InventoryTable,
    table_total: u64,
    mut checkpoint: InventoryCheckpoint,
    sequence: &mut u64,
    completed_before_table: u64,
    total_rows: u64,
    started: Instant,
) -> ProfilerResult<u64> {
    loop {
        let batch = load_batch(connection, table, &checkpoint, request.options.batch_size)?;
        if batch.is_empty() {
            if checkpoint.completed_rows != table_total {
                return Err(ProfilerError::IncompatibleSource(format!(
                    "{table} inventory ended at {} rows, expected {table_total}",
                    checkpoint.completed_rows
                )));
            }
            return Ok(checkpoint.completed_rows);
        }

        let batch_len = u64::try_from(batch.len())
            .map_err(|_| ProfilerError::Internal("inventory batch length overflowed u64".into()))?;
        checkpoint.completed_rows = checkpoint
            .completed_rows
            .checked_add(batch_len)
            .ok_or_else(|| ProfilerError::Internal("table progress overflowed u64".into()))?;
        *sequence = sequence.saturating_add(1);
        checkpoint.sequence = *sequence;
        update_checkpoint_key(&mut checkpoint, &batch)?;

        sink.ingest_batch(request, batch, &checkpoint)?;

        let completed_items = completed_before_table
            .checked_add(checkpoint.completed_rows)
            .ok_or_else(|| ProfilerError::Internal("inventory progress overflowed u64".into()))?;
        progress.send(ProgressEvent {
            run_id: request.run_id.clone(),
            sequence: *sequence,
            stage: RunStage::MetadataInventory,
            stage_state: StageState::Running,
            unit: ProgressUnit::Rows,
            completed_items,
            total_items: Some(total_rows),
            completed_bytes: 0,
            total_bytes: None,
            elapsed_ms: elapsed_ms(started),
            instant_throughput: None,
            smoothed_throughput: Some(rows_per_second(completed_items, started)),
            eta_ms: estimate_eta_ms(completed_items, total_rows, started),
            active_workers: 1,
            queue_depth: 0,
            warnings: 0,
            errors: 0,
            current_object_display: Some(format!(
                "{} · {} / {}",
                table, checkpoint.completed_rows, table_total
            )),
            checkpoint_sequence: checkpoint.sequence,
        })?;
    }
}

fn load_batch(
    connection: &Connection,
    table: InventoryTable,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    match table {
        InventoryTable::Messages => load_messages(connection, checkpoint, batch_size),
        InventoryTable::MessageOccurrences => load_occurrences(connection, checkpoint, batch_size),
        InventoryTable::Participants => load_participants(connection, checkpoint, batch_size),
        InventoryTable::Blobs => load_blobs(connection, checkpoint, batch_size),
        InventoryTable::Parts => load_parts(connection, checkpoint, batch_size),
        InventoryTable::MessageRelations => load_relations(connection, checkpoint, batch_size),
    }
}

fn load_messages(
    connection: &Connection,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    let mut statement = connection
        .prepare_cached(
            "SELECT id, archive_id, account_id, provider_thread_namespace, provider_thread_value, \
                    rfc_message_id, subject_raw, subject_normalized, header_date, raw_path, \
                    raw_sha256, raw_size_bytes, parse_defects_json \
             FROM messages WHERE id > ?1 ORDER BY id LIMIT ?2",
        )
        .map_err(|source| sqlite_error("preparing message inventory", source))?;
    let rows = statement
        .query_map(
            params![
                checkpoint.last_integer_key.unwrap_or_default(),
                i64::from(batch_size)
            ],
            map_message,
        )
        .map_err(|source| sqlite_error("querying message inventory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting message inventory", source))?;
    Ok(InventoryBatch::Messages(rows))
}

fn load_occurrences(
    connection: &Connection,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    let mut statement = connection
        .prepare_cached(
            "SELECT id, message_id, generation_id, uid, labels_json, internal_date, fetch_status \
             FROM message_occurrences WHERE id > ?1 ORDER BY id LIMIT ?2",
        )
        .map_err(|source| sqlite_error("preparing occurrence inventory", source))?;
    let rows = statement
        .query_map(
            params![
                checkpoint.last_integer_key.unwrap_or_default(),
                i64::from(batch_size)
            ],
            |row| {
                Ok(SourceOccurrenceRecord {
                    id: row.get(0)?,
                    message_id: row.get(1)?,
                    generation_id: row.get(2)?,
                    uid: row.get(3)?,
                    labels_json: row.get(4)?,
                    internal_date: row.get(5)?,
                    fetch_status: row.get(6)?,
                })
            },
        )
        .map_err(|source| sqlite_error("querying occurrence inventory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting occurrence inventory", source))?;
    Ok(InventoryBatch::MessageOccurrences(rows))
}

fn load_participants(
    connection: &Connection,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    let mut statement = connection
        .prepare_cached(
            "SELECT id, message_id, role, ordinal, name, address, domain \
             FROM message_participants WHERE id > ?1 ORDER BY id LIMIT ?2",
        )
        .map_err(|source| sqlite_error("preparing participant inventory", source))?;
    let rows = statement
        .query_map(
            params![
                checkpoint.last_integer_key.unwrap_or_default(),
                i64::from(batch_size)
            ],
            |row| {
                Ok(SourceParticipantRecord {
                    id: row.get(0)?,
                    message_id: row.get(1)?,
                    role: row.get(2)?,
                    ordinal: row.get(3)?,
                    name: row.get(4)?,
                    address: row.get(5)?,
                    domain: row.get(6)?,
                })
            },
        )
        .map_err(|source| sqlite_error("querying participant inventory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting participant inventory", source))?;
    Ok(InventoryBatch::Participants(rows))
}

fn load_blobs(
    connection: &Connection,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    let mut statement = connection
        .prepare_cached(
            "SELECT sha256, size_bytes, detected_mime_type, storage_path, first_seen_at, last_verified_at \
             FROM blobs WHERE sha256 > ?1 ORDER BY sha256 LIMIT ?2",
        )
        .map_err(|source| sqlite_error("preparing blob inventory", source))?;
    let rows = statement
        .query_map(
            params![
                checkpoint.last_text_key.as_deref().unwrap_or(""),
                i64::from(batch_size)
            ],
            |row| {
                Ok(SourceBlobRecord {
                    sha256: row.get(0)?,
                    size_bytes: non_negative_u64(row, 1, "blobs.size_bytes")?,
                    detected_mime_type: row.get(2)?,
                    storage_path: row.get(3)?,
                    first_seen_at: row.get(4)?,
                    last_verified_at: row.get(5)?,
                })
            },
        )
        .map_err(|source| sqlite_error("querying blob inventory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting blob inventory", source))?;
    Ok(InventoryBatch::Blobs(rows))
}

fn load_parts(
    connection: &Connection,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    let mut statement = connection
        .prepare_cached(
            "SELECT p.id, p.message_id, p.part_path, p.parent_part_path, p.role, p.declared_mime_type, \
                    p.detected_mime_type, p.content_disposition, p.content_id, p.filename_original, \
                    p.filename_safe, p.charset, p.transfer_encoding, p.size_bytes, p.sha256, p.blob_path, \
                    p.defects_json, m.header_date, sender.domain \
             FROM message_parts AS p \
             JOIN messages AS m ON m.id = p.message_id \
             LEFT JOIN message_participants AS sender \
               ON sender.message_id = p.message_id AND sender.role = 'from' AND sender.ordinal = 0 \
             WHERE p.id > ?1 ORDER BY p.id LIMIT ?2",
        )
        .map_err(|source| sqlite_error("preparing part inventory", source))?;
    let rows = statement
        .query_map(
            params![
                checkpoint.last_integer_key.unwrap_or_default(),
                i64::from(batch_size)
            ],
            |row| {
                Ok(SourcePartRecord {
                    id: row.get(0)?,
                    message_id: row.get(1)?,
                    part_path: row.get(2)?,
                    parent_part_path: row.get(3)?,
                    role: row.get(4)?,
                    declared_mime_type: row.get(5)?,
                    detected_mime_type: row.get(6)?,
                    content_disposition: row.get(7)?,
                    content_id: row.get(8)?,
                    filename_original: row.get(9)?,
                    filename_safe: row.get(10)?,
                    charset: row.get(11)?,
                    transfer_encoding: row.get(12)?,
                    size_bytes: non_negative_u64(row, 13, "message_parts.size_bytes")?,
                    sha256: row.get(14)?,
                    blob_path: row.get(15)?,
                    defects_json: row.get(16)?,
                    message_date: row.get(17)?,
                    sender_domain: row.get(18)?,
                })
            },
        )
        .map_err(|source| sqlite_error("querying part inventory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting part inventory", source))?;
    Ok(InventoryBatch::Parts(rows))
}

fn load_relations(
    connection: &Connection,
    checkpoint: &InventoryCheckpoint,
    batch_size: u32,
) -> ProfilerResult<InventoryBatch> {
    let mut statement = connection
        .prepare_cached(
            "SELECT id, source_message_id, target_message_id, relation_type, evidence_type, confidence, created_at \
             FROM message_relations WHERE id > ?1 ORDER BY id LIMIT ?2",
        )
        .map_err(|source| sqlite_error("preparing relation inventory", source))?;
    let rows = statement
        .query_map(
            params![
                checkpoint.last_integer_key.unwrap_or_default(),
                i64::from(batch_size)
            ],
            |row| {
                Ok(SourceRelationRecord {
                    id: row.get(0)?,
                    source_message_id: row.get(1)?,
                    target_message_id: row.get(2)?,
                    relation_type: row.get(3)?,
                    evidence_type: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .map_err(|source| sqlite_error("querying relation inventory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting relation inventory", source))?;
    Ok(InventoryBatch::MessageRelations(rows))
}

fn map_message(row: &Row<'_>) -> rusqlite::Result<SourceMessageRecord> {
    let raw_size: Option<i64> = row.get(11)?;
    let raw_size_bytes = raw_size
        .map(|value| {
            u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(11, value))
        })
        .transpose()?;
    Ok(SourceMessageRecord {
        id: row.get(0)?,
        archive_id: row.get(1)?,
        account_id: row.get(2)?,
        provider_thread_namespace: row.get(3)?,
        provider_thread_value: row.get(4)?,
        rfc_message_id: row.get(5)?,
        subject_raw: row.get(6)?,
        subject_normalized: row.get(7)?,
        header_date: row.get(8)?,
        raw_path: row.get(9)?,
        raw_sha256: row.get(10)?,
        raw_size_bytes,
        parse_defects_json: row.get(12)?,
    })
}

fn non_negative_u64(row: &Row<'_>, index: usize, field: &str) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            format!("{field} cannot be negative: {value}").into(),
        )
    })
}

fn update_checkpoint_key(
    checkpoint: &mut InventoryCheckpoint,
    batch: &InventoryBatch,
) -> ProfilerResult<()> {
    match batch {
        InventoryBatch::Messages(rows) => {
            checkpoint.last_integer_key = rows.last().map(|row| row.id);
        }
        InventoryBatch::MessageOccurrences(rows) => {
            checkpoint.last_integer_key = rows.last().map(|row| row.id);
        }
        InventoryBatch::Participants(rows) => {
            checkpoint.last_integer_key = rows.last().map(|row| row.id);
        }
        InventoryBatch::Blobs(rows) => {
            checkpoint.last_text_key = rows.last().map(|row| row.sha256.clone());
        }
        InventoryBatch::Parts(rows) => checkpoint.last_integer_key = rows.last().map(|row| row.id),
        InventoryBatch::MessageRelations(rows) => {
            checkpoint.last_integer_key = rows.last().map(|row| row.id);
        }
    }
    let valid = match checkpoint.table {
        InventoryTable::Blobs => checkpoint.last_text_key.is_some(),
        _ => checkpoint.last_integer_key.is_some(),
    };
    if !valid {
        return Err(ProfilerError::Internal(format!(
            "non-empty {} batch did not produce a checkpoint key",
            checkpoint.table
        )));
    }
    Ok(())
}

fn validate_request(request: &InventoryRequest) -> ProfilerResult<()> {
    if request.run_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument(
            "run_id cannot be empty".into(),
        ));
    }
    if request.collection_id.trim().is_empty() || request.source_snapshot_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument(
            "collection_id and source_snapshot_id cannot be empty".into(),
        ));
    }
    if request.options.batch_size == 0 {
        return Err(ProfilerError::InvalidArgument(
            "inventory batch_size must be greater than zero".into(),
        ));
    }
    if !request.snapshot_database.is_file() {
        return Err(ProfilerError::InvalidPath {
            message: "source snapshot database is missing".into(),
            path: request.snapshot_database.clone(),
        });
    }
    if !request.archive_root.is_dir() {
        return Err(ProfilerError::InvalidPath {
            message: "MailVault archive root is missing".into(),
            path: request.archive_root.clone(),
        });
    }
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[allow(clippy::cast_precision_loss)]
fn rows_per_second(completed_rows: u64, started: Instant) -> f64 {
    let seconds = started.elapsed().as_secs_f64();
    if seconds <= f64::EPSILON {
        0.0
    } else {
        completed_rows as f64 / seconds
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn estimate_eta_ms(completed: u64, total: u64, started: Instant) -> Option<u64> {
    if completed < 2_000 || completed >= total {
        return (completed >= total).then_some(0);
    }
    let rate = rows_per_second(completed, started);
    if !rate.is_finite() || rate <= f64::EPSILON {
        return None;
    }
    let remaining = total.saturating_sub(completed) as f64;
    let milliseconds = (remaining / rate) * 1_000.0;
    if milliseconds.is_finite() && milliseconds >= 0.0 {
        Some(milliseconds.round().min(u64::MAX as f64) as u64)
    } else {
        None
    }
}

#[allow(clippy::needless_pass_by_value)]
fn sqlite_error(operation: &'static str, source: rusqlite::Error) -> ProfilerError {
    ProfilerError::Sqlite {
        operation,
        message: source.to_string(),
    }
}

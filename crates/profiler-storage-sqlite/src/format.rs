use std::collections::BTreeMap;

use profiler_core::{
    ErrorCode, FormatMatch, FormatObjectRow, FormatObservation, FormatPage, FormatPageRequest,
    FormatRunRegistration, FormatRunStartRequest, FormatState, FormatSummary, FormatToolIdentity,
    FormatWorkItem, ProfilerError, ProfilerResult,
};
use rusqlite::{
    OptionalExtension, Transaction, TransactionBehavior, named_params, params, types::Type,
};
use uuid::Uuid;

use super::{ProfilerStore, now_text, sqlite_error, to_u64};

impl ProfilerStore {
    pub fn begin_format_run(
        &mut self,
        request: &FormatRunStartRequest<'_>,
    ) -> ProfilerResult<FormatRunRegistration> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sqlite_error("starting format run transaction", source))?;

        let collection_id = load_format_baseline(&transaction, request.baseline_run_id)?;
        let resumable = find_resumable_format_run(&transaction, request)?;
        let tool_id = register_tool(&transaction, request.tool)?;
        let workload = load_format_workload(&transaction, &collection_id)?;
        let registration = match resumable {
            Some(candidate) => {
                resume_format_run(&transaction, &collection_id, &tool_id, workload, candidate)?
            }
            None => create_format_run(&transaction, request, &collection_id, &tool_id, workload)?,
        };

        transaction
            .commit()
            .map_err(|source| sqlite_error("committing exact-format run registration", source))?;
        Ok(registration)
    }

    pub fn load_format_work_batch(
        &self,
        collection_id: &str,
        after_sha256: Option<&str>,
        limit: u32,
    ) -> ProfilerResult<Vec<FormatWorkItem>> {
        if limit == 0 {
            return Err(ProfilerError::InvalidArgument(
                "format batch limit must be greater than zero".into(),
            ));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT content.id, content.sha256, content.expected_size_bytes,
                        content.source_detected_mime_type, content.canonical_path_display,
                        content.availability_state,
                        (SELECT variant.display_filename FROM filename_variants AS variant
                         WHERE variant.content_object_id=content.id
                         ORDER BY variant.occurrence_count DESC, variant.normalized_filename
                         LIMIT 1)
                 FROM content_objects AS content
                 WHERE content.collection_id=:collection_id
                   AND (:after_sha256 IS NULL OR content.sha256 > :after_sha256)
                 ORDER BY content.sha256
                 LIMIT :limit",
            )
            .map_err(|source| sqlite_error("preparing exact-format workload batch", source))?;
        statement
            .query_map(
                named_params! {
                    ":collection_id": collection_id,
                    ":after_sha256": after_sha256,
                    ":limit": i64::from(limit),
                },
                |row| {
                    let filename: Option<String> = row.get(6)?;
                    Ok(FormatWorkItem {
                        content_object_id: row.get(0)?,
                        sha256: row.get(1)?,
                        expected_size_bytes: row_u64(row, 2)?,
                        source_mime_type: row.get(3)?,
                        canonical_path_display: row.get(4)?,
                        availability_state: row.get(5)?,
                        preferred_extension: filename.as_deref().and_then(safe_extension),
                    })
                },
            )
            .map_err(|source| sqlite_error("querying exact-format workload batch", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting exact-format workload batch", source))
    }

    pub fn commit_format_observations(
        &mut self,
        format_run_id: &str,
        baseline_run_id: &str,
        observations: &[FormatObservation],
        checkpoint_sha256: &str,
        checkpoint_sequence: u64,
    ) -> ProfilerResult<FormatSummary> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sqlite_error("starting exact-format result transaction", source))?;

        ensure_format_run_active(&transaction, format_run_id, baseline_run_id)?;
        for observation in observations {
            persist_format_observation(&transaction, format_run_id, baseline_run_id, observation)?;
        }
        refresh_format_run_projection(
            &transaction,
            format_run_id,
            checkpoint_sha256,
            checkpoint_sequence,
        )?;
        transaction
            .commit()
            .map_err(|source| sqlite_error("committing exact-format result batch", source))?;
        self.format_summary(baseline_run_id)
    }

    pub fn complete_format_run(
        &mut self,
        format_run_id: &str,
        baseline_run_id: &str,
    ) -> ProfilerResult<FormatSummary> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| {
                sqlite_error("starting exact-format completion transaction", source)
            })?;
        let (completed, total): (i64, i64) = transaction
            .query_row(
                "SELECT completed_objects, total_objects FROM format_identification_runs
                 WHERE id=?1 AND baseline_run_id=?2 AND state='running'",
                params![format_run_id, baseline_run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|source| sqlite_error("validating exact-format completion", source))?
            .ok_or_else(|| {
                ProfilerError::contract(
                    ErrorCode::FormatRunNotFound,
                    "active exact-format run was not found",
                    false,
                )
            })?;
        if completed != total {
            return Err(ProfilerError::contract_with_context(
                ErrorCode::FormatRunFailed,
                "exact-format run cannot be completed before every content object is accounted for",
                true,
                BTreeMap::from([
                    ("completedObjects".into(), completed.to_string()),
                    ("totalObjects".into(), total.to_string()),
                ]),
            ));
        }
        transaction
            .execute(
                "UPDATE format_identification_runs
                 SET state='succeeded', finished_at=?1
                 WHERE id=?2 AND baseline_run_id=?3 AND state='running'",
                params![now_text(), format_run_id, baseline_run_id],
            )
            .map_err(|source| sqlite_error("completing exact-format run", source))?;
        transaction
            .commit()
            .map_err(|source| sqlite_error("committing exact-format completion", source))?;
        self.format_summary(baseline_run_id)
    }

    pub fn fail_format_run(
        &mut self,
        format_run_id: &str,
        error: &ProfilerError,
    ) -> ProfilerResult<()> {
        let report = error.report();
        self.connection
            .execute(
                "UPDATE format_identification_runs SET
                    state='failed', finished_at=?1, failure_code=?2, failure_message=?3
                 WHERE id=?4 AND state='running'",
                params![
                    now_text(),
                    format!("{:?}", report.code).to_ascii_lowercase(),
                    report.message,
                    format_run_id,
                ],
            )
            .map_err(|source| sqlite_error("failing exact-format run", source))?;
        Ok(())
    }

    pub fn format_summary(&self, baseline_run_id: &str) -> ProfilerResult<FormatSummary> {
        let collection_id = load_summary_collection_id(&self.connection, baseline_run_id)?;
        let workload = load_format_workload(&self.connection, &collection_id)?;
        let latest = load_latest_format_run(&self.connection, baseline_run_id)?;
        let distinct_puids = count_distinct_puids(&self.connection, baseline_run_id)?;

        let mut summary = FormatSummary {
            baseline_run_id: baseline_run_id.into(),
            total_objects: workload.total_objects()?,
            eligible_objects: workload.eligible_objects()?,
            total_bytes: workload.total_bytes()?,
            distinct_puids,
            ..FormatSummary::default()
        };
        if let Some(latest) = latest {
            latest.apply_to(&mut summary)?;
        }
        Ok(summary)
    }

    pub fn format_page(&self, request: &FormatPageRequest) -> ProfilerResult<FormatPage> {
        validate_format_page_request(request)?;
        let collection_id: String = self
            .connection
            .query_row(
                "SELECT collection_id FROM profiler_runs WHERE id=?1",
                [&request.baseline_run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| sqlite_error("loading format page collection", source))?
            .ok_or_else(|| {
                ProfilerError::contract(ErrorCode::RunNotFound, "baseline run was not found", false)
            })?;
        let search_like = request
            .filters
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()));
        let state = request.filters.state.map(FormatState::as_str);
        let puid = request
            .filters
            .puid
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let mismatch = i64::from(request.filters.mismatch_only);
        let total_i64: i64 = self
            .connection
            .query_row(
                format_count_sql(),
                named_params! {
                    ":baseline_run_id": request.baseline_run_id.as_str(),
                    ":collection_id": collection_id.as_str(),
                    ":search_like": search_like,
                    ":state": state,
                    ":puid": puid,
                    ":mismatch_only": mismatch,
                },
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("counting exact-format page", source))?;
        let fetch_limit = i64::from(request.limit) + 1;
        let mut statement = self
            .connection
            .prepare(format_page_sql())
            .map_err(|source| sqlite_error("preparing exact-format page", source))?;
        let mut items = statement
            .query_map(
                named_params! {
                    ":baseline_run_id": request.baseline_run_id.as_str(),
                    ":collection_id": collection_id.as_str(),
                    ":search_like": search_like,
                    ":state": state,
                    ":puid": puid,
                    ":mismatch_only": mismatch,
                    ":after_sha256": request.after_sha256.as_deref(),
                    ":fetch_limit": fetch_limit,
                },
                format_row,
            )
            .map_err(|source| sqlite_error("querying exact-format page", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting exact-format page", source))?;
        let has_more = items.len() > request.limit as usize;
        if has_more {
            items.pop();
        }
        let next_after_sha256 = if has_more {
            items.last().map(|item| item.sha256.clone())
        } else {
            None
        };
        Ok(FormatPage {
            items,
            total_filtered: to_u64(total_i64, "format filtered total")?,
            next_after_sha256,
            has_more,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct FormatWorkload {
    total_objects: i64,
    eligible_objects: i64,
    total_bytes: i64,
}

impl FormatWorkload {
    fn total_objects(self) -> ProfilerResult<u64> {
        to_u64(self.total_objects, "format total object count")
    }

    fn eligible_objects(self) -> ProfilerResult<u64> {
        to_u64(self.eligible_objects, "format eligible object count")
    }

    fn total_bytes(self) -> ProfilerResult<u64> {
        to_u64(self.total_bytes, "format total byte count")
    }
}

#[derive(Debug)]
struct ActiveFormatRun {
    id: String,
    configuration_fingerprint: String,
    checkpoint_sha256: Option<String>,
    completed_objects: i64,
    completed_bytes: i64,
    checkpoint_sequence: i64,
}

#[derive(Debug)]
struct ResumableFormatRun {
    id: String,
    checkpoint_sha256: Option<String>,
    completed_objects: i64,
    completed_bytes: i64,
    checkpoint_sequence: i64,
}

#[derive(Debug, Default)]
struct FormatRunProgress {
    checkpoint_sha256: Option<String>,
    completed_objects: i64,
    completed_bytes: i64,
    checkpoint_sequence: i64,
}

impl From<ActiveFormatRun> for ResumableFormatRun {
    fn from(active: ActiveFormatRun) -> Self {
        Self {
            id: active.id,
            checkpoint_sha256: active.checkpoint_sha256,
            completed_objects: active.completed_objects,
            completed_bytes: active.completed_bytes,
            checkpoint_sequence: active.checkpoint_sequence,
        }
    }
}

impl ResumableFormatRun {
    fn into_parts(self) -> (String, FormatRunProgress) {
        (
            self.id,
            FormatRunProgress {
                checkpoint_sha256: self.checkpoint_sha256,
                completed_objects: self.completed_objects,
                completed_bytes: self.completed_bytes,
                checkpoint_sequence: self.checkpoint_sequence,
            },
        )
    }
}

#[derive(Debug)]
struct PersistedFormatSummary {
    format_run_id: String,
    state: String,
    completed_objects: i64,
    completed_bytes: i64,
    identified: i64,
    unknown: i64,
    ambiguous: i64,
    empty: i64,
    skipped_unavailable: i64,
    tool_errors: i64,
    extension_mismatches: i64,
    started_at: String,
    finished_at: Option<String>,
    tool_name: String,
    tool_version: String,
    executable_path: String,
    executable_sha256: String,
    signature_path: String,
    signature_sha256: String,
    signature_version: String,
    signature_created: Option<String>,
    identifiers_json: String,
    probed_at: String,
}

impl PersistedFormatSummary {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            format_run_id: row.get(0)?,
            state: row.get(1)?,
            completed_objects: row.get(2)?,
            completed_bytes: row.get(3)?,
            identified: row.get(4)?,
            unknown: row.get(5)?,
            ambiguous: row.get(6)?,
            empty: row.get(7)?,
            skipped_unavailable: row.get(8)?,
            tool_errors: row.get(9)?,
            extension_mismatches: row.get(10)?,
            started_at: row.get(11)?,
            finished_at: row.get(12)?,
            tool_name: row.get(13)?,
            tool_version: row.get(14)?,
            executable_path: row.get(15)?,
            executable_sha256: row.get(16)?,
            signature_path: row.get(17)?,
            signature_sha256: row.get(18)?,
            signature_version: row.get(19)?,
            signature_created: row.get(20)?,
            identifiers_json: row.get(21)?,
            probed_at: row.get(22)?,
        })
    }

    fn apply_to(self, summary: &mut FormatSummary) -> ProfilerResult<()> {
        summary.latest_format_run_id = Some(self.format_run_id);
        summary.latest_run_state = Some(self.state);
        summary.completed_objects = to_u64(self.completed_objects, "format completed objects")?;
        summary.completed_bytes = to_u64(self.completed_bytes, "format completed bytes")?;
        summary.identified = to_u64(self.identified, "identified formats")?;
        summary.unknown = to_u64(self.unknown, "unknown formats")?;
        summary.ambiguous = to_u64(self.ambiguous, "ambiguous formats")?;
        summary.empty = to_u64(self.empty, "empty format objects")?;
        summary.skipped_unavailable = to_u64(self.skipped_unavailable, "skipped format objects")?;
        summary.tool_errors = to_u64(self.tool_errors, "format tool errors")?;
        summary.extension_mismatches =
            to_u64(self.extension_mismatches, "format extension mismatches")?;
        summary.started_at = Some(self.started_at);
        summary.finished_at = self.finished_at;
        summary.tool = Some(FormatToolIdentity {
            tool_name: self.tool_name,
            tool_version: self.tool_version,
            executable_path: self.executable_path,
            executable_sha256: self.executable_sha256,
            signature_path: self.signature_path,
            signature_sha256: if self.signature_sha256.is_empty() {
                None
            } else {
                Some(self.signature_sha256)
            },
            signature_version: self.signature_version,
            signature_created: self.signature_created,
            identifiers: serde_json::from_str(&self.identifiers_json).map_err(|error| {
                ProfilerError::Internal(format!("parsing persisted format identifiers: {error}"))
            })?,
            probed_at: self.probed_at,
        });
        Ok(())
    }
}

fn load_format_baseline(
    transaction: &Transaction<'_>,
    baseline_run_id: &str,
) -> ProfilerResult<String> {
    let (collection_id, state): (String, String) = transaction
        .query_row(
            "SELECT collection_id, state FROM profiler_runs WHERE id=?1",
            [baseline_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|source| sqlite_error("loading baseline profiler run", source))?
        .ok_or_else(|| {
            ProfilerError::contract(ErrorCode::RunNotFound, "baseline run was not found", false)
        })?;
    if state != "succeeded" {
        return Err(ProfilerError::contract(
            ErrorCode::RunNotBrowsable,
            "exact format identification requires a completed physical profile",
            false,
        ));
    }
    Ok(collection_id)
}

fn find_resumable_format_run(
    transaction: &Transaction<'_>,
    request: &FormatRunStartRequest<'_>,
) -> ProfilerResult<Option<ResumableFormatRun>> {
    if let Some(active) = load_active_format_run(transaction, request.baseline_run_id)? {
        if request.resume && active.configuration_fingerprint == request.configuration_fingerprint {
            return Ok(Some(active.into()));
        }
        return Err(ProfilerError::contract_with_context(
            ErrorCode::FormatRunAlreadyActive,
            "an exact-format run is already active for this physical profile",
            true,
            BTreeMap::from([("formatRunId".into(), active.id)]),
        ));
    }
    if !request.resume {
        return Ok(None);
    }
    load_failed_resumable_format_run(
        transaction,
        request.baseline_run_id,
        request.configuration_fingerprint,
    )
}

fn load_active_format_run(
    transaction: &Transaction<'_>,
    baseline_run_id: &str,
) -> ProfilerResult<Option<ActiveFormatRun>> {
    transaction
        .query_row(
            "SELECT id, configuration_fingerprint, checkpoint_sha256,
                    completed_objects, completed_bytes, checkpoint_sequence
             FROM format_identification_runs
             WHERE baseline_run_id=?1 AND state='running'
             ORDER BY started_at DESC LIMIT 1",
            [baseline_run_id],
            |row| {
                Ok(ActiveFormatRun {
                    id: row.get(0)?,
                    configuration_fingerprint: row.get(1)?,
                    checkpoint_sha256: row.get(2)?,
                    completed_objects: row.get(3)?,
                    completed_bytes: row.get(4)?,
                    checkpoint_sequence: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|source| sqlite_error("checking active format run", source))
}

fn load_failed_resumable_format_run(
    transaction: &Transaction<'_>,
    baseline_run_id: &str,
    configuration_fingerprint: &str,
) -> ProfilerResult<Option<ResumableFormatRun>> {
    transaction
        .query_row(
            "SELECT id, checkpoint_sha256, completed_objects, completed_bytes,
                    checkpoint_sequence
             FROM format_identification_runs
             WHERE baseline_run_id=?1 AND configuration_fingerprint=?2
               AND state IN ('failed', 'cancelled')
             ORDER BY started_at DESC LIMIT 1",
            params![baseline_run_id, configuration_fingerprint],
            |row| {
                Ok(ResumableFormatRun {
                    id: row.get(0)?,
                    checkpoint_sha256: row.get(1)?,
                    completed_objects: row.get(2)?,
                    completed_bytes: row.get(3)?,
                    checkpoint_sequence: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|source| sqlite_error("finding resumable format run", source))
}

fn load_format_workload(
    connection: &rusqlite::Connection,
    collection_id: &str,
) -> ProfilerResult<FormatWorkload> {
    connection
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN availability_state='available'
                                      AND expected_size_bytes>0 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN availability_state='available'
                                      AND expected_size_bytes>0
                                      THEN expected_size_bytes ELSE 0 END), 0)
             FROM content_objects WHERE collection_id=?1",
            [collection_id],
            |row| {
                Ok(FormatWorkload {
                    total_objects: row.get(0)?,
                    eligible_objects: row.get(1)?,
                    total_bytes: row.get(2)?,
                })
            },
        )
        .map_err(|source| sqlite_error("counting exact-format workload", source))
}

fn resume_format_run(
    transaction: &Transaction<'_>,
    collection_id: &str,
    tool_id: &str,
    workload: FormatWorkload,
    candidate: ResumableFormatRun,
) -> ProfilerResult<FormatRunRegistration> {
    transaction
        .execute(
            "UPDATE format_identification_runs SET
                state='running', tool_id=?1, failure_code=NULL, failure_message=NULL,
                finished_at=NULL
             WHERE id=?2",
            params![tool_id, candidate.id.as_str()],
        )
        .map_err(|source| sqlite_error("resuming exact-format run", source))?;
    let (format_run_id, progress) = candidate.into_parts();
    build_format_registration(format_run_id, collection_id, workload, progress)
}

fn create_format_run(
    transaction: &Transaction<'_>,
    request: &FormatRunStartRequest<'_>,
    collection_id: &str,
    tool_id: &str,
    workload: FormatWorkload,
) -> ProfilerResult<FormatRunRegistration> {
    let id = Uuid::now_v7().to_string();
    transaction
        .execute(
            "INSERT INTO format_identification_runs(
                id, baseline_run_id, collection_id, tool_id, state,
                configuration_fingerprint, batch_size, worker_count, timeout_seconds,
                total_objects, eligible_objects, total_bytes, started_at
             ) VALUES(?1, ?2, ?3, ?4, 'running', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id.as_str(),
                request.baseline_run_id,
                collection_id,
                tool_id,
                request.configuration_fingerprint,
                i64::from(request.batch_size),
                i64::from(request.worker_count),
                i64::try_from(request.timeout_seconds).unwrap_or(i64::MAX),
                workload.total_objects,
                workload.eligible_objects,
                workload.total_bytes,
                now_text(),
            ],
        )
        .map_err(|source| sqlite_error("creating exact-format run", source))?;
    build_format_registration(id, collection_id, workload, FormatRunProgress::default())
}

fn build_format_registration(
    format_run_id: String,
    collection_id: &str,
    workload: FormatWorkload,
    progress: FormatRunProgress,
) -> ProfilerResult<FormatRunRegistration> {
    Ok(FormatRunRegistration {
        format_run_id,
        collection_id: collection_id.into(),
        total_objects: workload.total_objects()?,
        eligible_objects: workload.eligible_objects()?,
        total_bytes: workload.total_bytes()?,
        resume_after_sha256: progress.checkpoint_sha256,
        completed_objects: to_u64(progress.completed_objects, "resumed completed object count")?,
        completed_bytes: to_u64(progress.completed_bytes, "resumed completed byte count")?,
        checkpoint_sequence: to_u64(progress.checkpoint_sequence, "resumed checkpoint sequence")?,
    })
}

fn persist_format_observation(
    transaction: &Transaction<'_>,
    format_run_id: &str,
    baseline_run_id: &str,
    observation: &FormatObservation,
) -> ProfilerResult<()> {
    let observation_id =
        upsert_format_observation(transaction, format_run_id, baseline_run_id, observation)?;
    replace_format_matches(transaction, &observation_id, &observation.matches)?;
    update_content_format_projection(transaction, format_run_id, observation)
}

fn upsert_format_observation(
    transaction: &Transaction<'_>,
    format_run_id: &str,
    baseline_run_id: &str,
    observation: &FormatObservation,
) -> ProfilerResult<String> {
    let observation_id = Uuid::now_v7().to_string();
    transaction
        .execute(
            "INSERT INTO format_observations(
                id, format_run_id, baseline_run_id, content_object_id, sha256, state,
                source_mime_type, preferred_extension, staging_mode, primary_identifier,
                primary_format_name, primary_format_version, primary_mime_type, match_count,
                extension_checked, extension_mismatch, error_code, error_message, observed_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
             ON CONFLICT(format_run_id, content_object_id) DO UPDATE SET
                sha256=excluded.sha256,
                state=excluded.state,
                source_mime_type=excluded.source_mime_type,
                preferred_extension=excluded.preferred_extension,
                staging_mode=excluded.staging_mode,
                primary_identifier=excluded.primary_identifier,
                primary_format_name=excluded.primary_format_name,
                primary_format_version=excluded.primary_format_version,
                primary_mime_type=excluded.primary_mime_type,
                match_count=excluded.match_count,
                extension_checked=excluded.extension_checked,
                extension_mismatch=excluded.extension_mismatch,
                error_code=excluded.error_code,
                error_message=excluded.error_message,
                observed_at=excluded.observed_at",
            params![
                observation_id,
                format_run_id,
                baseline_run_id,
                observation.content_object_id,
                observation.sha256,
                observation.state.as_str(),
                observation.source_mime_type,
                observation.preferred_extension,
                observation.staging_mode,
                observation.primary_identifier,
                observation.primary_format_name,
                observation.primary_format_version,
                observation.primary_mime_type,
                i64::try_from(observation.match_count).unwrap_or(i64::MAX),
                i64::from(observation.extension_checked),
                i64::from(observation.extension_mismatch),
                observation.error_code,
                observation.error_message,
                observation.observed_at,
            ],
        )
        .map_err(|source| sqlite_error("upserting exact-format observation", source))?;
    transaction
        .query_row(
            "SELECT id FROM format_observations
             WHERE format_run_id=?1 AND content_object_id=?2",
            params![format_run_id, observation.content_object_id],
            |row| row.get(0),
        )
        .map_err(|source| sqlite_error("reading exact-format observation id", source))
}

fn replace_format_matches(
    transaction: &Transaction<'_>,
    observation_id: &str,
    matches: &[FormatMatch],
) -> ProfilerResult<()> {
    transaction
        .execute(
            "DELETE FROM format_matches WHERE observation_id=?1",
            [observation_id],
        )
        .map_err(|source| sqlite_error("replacing exact-format matches", source))?;
    for (ordinal, format_match) in matches.iter().enumerate() {
        insert_match(transaction, observation_id, ordinal, format_match)?;
    }
    Ok(())
}

fn update_content_format_projection(
    transaction: &Transaction<'_>,
    format_run_id: &str,
    observation: &FormatObservation,
) -> ProfilerResult<()> {
    transaction
        .execute(
            "UPDATE content_objects SET
                format_state=?1,
                primary_puid=?2,
                primary_format_name=?3,
                primary_format_version=?4,
                primary_format_mime_type=?5,
                format_match_count=?6,
                extension_checked=?7,
                extension_mismatch=?8,
                last_format_run_id=?9,
                last_format_at=?10,
                updated_at=?10
             WHERE id=?11",
            params![
                observation.state.as_str(),
                observation.primary_identifier,
                observation.primary_format_name,
                observation.primary_format_version,
                observation.primary_mime_type,
                i64::try_from(observation.match_count).unwrap_or(i64::MAX),
                i64::from(observation.extension_checked),
                i64::from(observation.extension_mismatch),
                format_run_id,
                observation.observed_at,
                observation.content_object_id,
            ],
        )
        .map_err(|source| sqlite_error("updating exact-format projection", source))?;
    Ok(())
}

fn load_summary_collection_id(
    connection: &rusqlite::Connection,
    baseline_run_id: &str,
) -> ProfilerResult<String> {
    connection
        .query_row(
            "SELECT collection_id FROM profiler_runs WHERE id=?1",
            [baseline_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|source| sqlite_error("loading format summary baseline run", source))?
        .ok_or_else(|| {
            ProfilerError::contract(ErrorCode::RunNotFound, "baseline run was not found", false)
        })
}

fn load_latest_format_run(
    connection: &rusqlite::Connection,
    baseline_run_id: &str,
) -> ProfilerResult<Option<PersistedFormatSummary>> {
    connection
        .query_row(
            "SELECT run.id, run.state, run.completed_objects, run.completed_bytes,
                    run.identified, run.unknown, run.ambiguous, run.empty_objects,
                    run.skipped_unavailable, run.tool_errors, run.extension_mismatches,
                    run.started_at, run.finished_at,
                    tool.tool_name, tool.tool_version, tool.executable_path,
                    tool.executable_sha256, tool.signature_path, tool.signature_sha256,
                    tool.signature_version, tool.signature_created, tool.identifiers_json,
                    tool.probed_at
             FROM format_identification_runs AS run
             JOIN format_tools AS tool ON tool.id=run.tool_id
             WHERE run.baseline_run_id=?1
             ORDER BY run.started_at DESC LIMIT 1",
            [baseline_run_id],
            PersistedFormatSummary::from_row,
        )
        .optional()
        .map_err(|source| sqlite_error("loading latest exact-format run", source))
}

fn count_distinct_puids(
    connection: &rusqlite::Connection,
    baseline_run_id: &str,
) -> ProfilerResult<u64> {
    let count = connection
        .query_row(
            "SELECT COUNT(DISTINCT primary_identifier)
             FROM format_observations
             WHERE format_run_id=(SELECT id FROM format_identification_runs
                                  WHERE baseline_run_id=?1
                                  ORDER BY started_at DESC LIMIT 1)
               AND primary_identifier IS NOT NULL",
            [baseline_run_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|source| sqlite_error("counting distinct format identifiers", source))?;
    to_u64(count, "distinct format identifier count")
}

fn register_tool(
    transaction: &rusqlite::Transaction<'_>,
    tool: &FormatToolIdentity,
) -> ProfilerResult<String> {
    let signature_sha256 = tool.signature_sha256.clone().unwrap_or_default();
    let existing = transaction
        .query_row(
            "SELECT id FROM format_tools
             WHERE tool_name=?1 AND executable_sha256=?2
               AND signature_version=?3 AND signature_sha256=?4",
            params![
                tool.tool_name,
                tool.executable_sha256,
                tool.signature_version,
                signature_sha256,
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|source| sqlite_error("finding persisted format tool", source))?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = Uuid::now_v7().to_string();
    let identifiers = serde_json::to_string(&tool.identifiers).map_err(|error| {
        ProfilerError::Internal(format!("serializing format tool identifiers: {error}"))
    })?;
    transaction
        .execute(
            "INSERT INTO format_tools(
                id, tool_name, tool_version, executable_path, executable_sha256,
                signature_path, signature_sha256, signature_version, signature_created,
                identifiers_json, probed_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                tool.tool_name,
                tool.tool_version,
                tool.executable_path,
                tool.executable_sha256,
                tool.signature_path,
                signature_sha256,
                tool.signature_version,
                tool.signature_created,
                identifiers,
                tool.probed_at,
            ],
        )
        .map_err(|source| sqlite_error("persisting format tool identity", source))?;
    Ok(id)
}

fn ensure_format_run_active(
    transaction: &rusqlite::Transaction<'_>,
    format_run_id: &str,
    baseline_run_id: &str,
) -> ProfilerResult<()> {
    let active: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM format_identification_runs
                            WHERE id=?1 AND baseline_run_id=?2 AND state='running')",
            params![format_run_id, baseline_run_id],
            |row| row.get(0),
        )
        .map_err(|source| sqlite_error("validating active exact-format run", source))?;
    if active {
        Ok(())
    } else {
        Err(ProfilerError::contract(
            ErrorCode::FormatRunNotFound,
            "active exact-format run was not found",
            false,
        ))
    }
}

fn insert_match(
    transaction: &rusqlite::Transaction<'_>,
    observation_id: &str,
    ordinal: usize,
    format_match: &FormatMatch,
) -> ProfilerResult<()> {
    transaction
        .execute(
            "INSERT INTO format_matches(
                id, observation_id, ordinal, namespace, identifier, format_name,
                format_version, mime_type, format_class, basis, warning, is_primary
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                Uuid::now_v7().to_string(),
                observation_id,
                i64::try_from(ordinal).unwrap_or(i64::MAX),
                format_match.namespace,
                format_match.identifier,
                format_match.format_name,
                format_match.format_version,
                format_match.mime_type,
                format_match.format_class,
                format_match.basis,
                format_match.warning,
                i64::from(format_match.is_primary),
            ],
        )
        .map_err(|source| sqlite_error("persisting exact-format match", source))?;
    Ok(())
}

fn refresh_format_run_projection(
    transaction: &rusqlite::Transaction<'_>,
    format_run_id: &str,
    checkpoint_sha256: &str,
    checkpoint_sequence: u64,
) -> ProfilerResult<()> {
    transaction
        .execute(
            "UPDATE format_identification_runs SET
                completed_objects=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1),
                completed_bytes=(SELECT COALESCE(SUM(CASE
                                     WHEN content.availability_state='available'
                                      AND content.expected_size_bytes>0
                                     THEN content.expected_size_bytes ELSE 0 END), 0)
                                 FROM format_observations AS observation
                                 JOIN content_objects AS content ON content.id=observation.content_object_id
                                 WHERE observation.format_run_id=?1),
                identified=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND state='identified'),
                unknown=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND state='unknown'),
                ambiguous=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND state='ambiguous'),
                empty_objects=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND state='empty'),
                skipped_unavailable=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND state='skipped_unavailable'),
                tool_errors=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND state='tool_error'),
                extension_mismatches=(SELECT COUNT(*) FROM format_observations WHERE format_run_id=?1 AND extension_mismatch=1),
                checkpoint_sha256=?2,
                checkpoint_sequence=?3
             WHERE id=?1",
            params![
                format_run_id,
                checkpoint_sha256,
                i64::try_from(checkpoint_sequence).unwrap_or(i64::MAX),
            ],
        )
        .map_err(|source| sqlite_error("refreshing exact-format run projection", source))?;
    Ok(())
}

fn safe_extension(filename: &str) -> Option<String> {
    let extension = std::path::Path::new(filename).extension()?.to_str()?;
    let cleaned = extension
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn validate_format_page_request(request: &FormatPageRequest) -> ProfilerResult<()> {
    if request.baseline_run_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument(
            "baseline run id cannot be empty".into(),
        ));
    }
    if request.limit == 0 || request.limit > 500 {
        return Err(ProfilerError::InvalidArgument(
            "format page limit must be between 1 and 500".into(),
        ));
    }
    Ok(())
}

fn parse_format_state(value: &str, column: usize) -> rusqlite::Result<FormatState> {
    match value {
        "uninspected" => Ok(FormatState::Uninspected),
        "identified" => Ok(FormatState::Identified),
        "unknown" => Ok(FormatState::Unknown),
        "ambiguous" => Ok(FormatState::Ambiguous),
        "empty" => Ok(FormatState::Empty),
        "skipped_unavailable" => Ok(FormatState::SkippedUnavailable),
        "tool_error" => Ok(FormatState::ToolError),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown format state: {other}"),
            )),
        )),
    }
}

fn row_u64(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(column)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
}

fn format_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FormatObjectRow> {
    let state: String = row.get(5)?;
    Ok(FormatObjectRow {
        content_object_id: row.get(0)?,
        sha256: row.get(1)?,
        primary_filename: row.get(2)?,
        expected_size_bytes: row_u64(row, 3)?,
        source_mime_type: row.get(4)?,
        state: parse_format_state(&state, 5)?,
        primary_identifier: row.get(6)?,
        primary_format_name: row.get(7)?,
        primary_format_version: row.get(8)?,
        primary_mime_type: row.get(9)?,
        match_count: row_u64(row, 10)?,
        extension_checked: row.get::<_, i64>(11)? != 0,
        extension_mismatch: row.get::<_, i64>(12)? != 0,
    })
}

fn format_count_sql() -> &'static str {
    "WITH latest_run AS (
         SELECT id FROM format_identification_runs
         WHERE baseline_run_id=:baseline_run_id
         ORDER BY started_at DESC LIMIT 1
     )
     SELECT COUNT(*) FROM content_objects AS content
     LEFT JOIN format_observations AS observation
       ON observation.content_object_id=content.id
      AND observation.format_run_id=(SELECT id FROM latest_run)
     WHERE content.collection_id=:collection_id
       AND (:state IS NULL OR COALESCE(observation.state, 'uninspected')=:state)
       AND (:puid IS NULL OR observation.primary_identifier=:puid)
       AND (:mismatch_only=0 OR COALESCE(observation.extension_mismatch, 0)=1)
       AND (:search_like IS NULL OR lower(content.sha256) LIKE :search_like
            OR lower(content.source_detected_mime_type) LIKE :search_like
            OR lower(COALESCE(observation.primary_identifier, '')) LIKE :search_like
            OR lower(COALESCE(observation.primary_format_name, '')) LIKE :search_like
            OR EXISTS(SELECT 1 FROM filename_variants AS variant
                      WHERE variant.content_object_id=content.id
                        AND lower(variant.display_filename) LIKE :search_like))"
}

fn format_page_sql() -> &'static str {
    "WITH latest_run AS (
         SELECT id FROM format_identification_runs
         WHERE baseline_run_id=:baseline_run_id
         ORDER BY started_at DESC LIMIT 1
     )
     SELECT content.id, content.sha256,
            COALESCE((SELECT variant.display_filename FROM filename_variants AS variant
                      WHERE variant.content_object_id=content.id
                      ORDER BY variant.occurrence_count DESC, variant.normalized_filename LIMIT 1),
                     '[unnamed]'),
            content.expected_size_bytes, content.source_detected_mime_type,
            COALESCE(observation.state, 'uninspected'), observation.primary_identifier,
            observation.primary_format_name, observation.primary_format_version,
            observation.primary_mime_type, COALESCE(observation.match_count, 0),
            COALESCE(observation.extension_checked, 0),
            COALESCE(observation.extension_mismatch, 0)
     FROM content_objects AS content
     LEFT JOIN format_observations AS observation
       ON observation.content_object_id=content.id
      AND observation.format_run_id=(SELECT id FROM latest_run)
     WHERE content.collection_id=:collection_id
       AND (:after_sha256 IS NULL OR content.sha256>:after_sha256)
       AND (:state IS NULL OR COALESCE(observation.state, 'uninspected')=:state)
       AND (:puid IS NULL OR observation.primary_identifier=:puid)
       AND (:mismatch_only=0 OR COALESCE(observation.extension_mismatch, 0)=1)
       AND (:search_like IS NULL OR lower(content.sha256) LIKE :search_like
            OR lower(content.source_detected_mime_type) LIKE :search_like
            OR lower(COALESCE(observation.primary_identifier, '')) LIKE :search_like
            OR lower(COALESCE(observation.primary_format_name, '')) LIKE :search_like
            OR EXISTS(SELECT 1 FROM filename_variants AS variant
                      WHERE variant.content_object_id=content.id
                        AND lower(variant.display_filename) LIKE :search_like))
     ORDER BY content.sha256 LIMIT :fetch_limit"
}

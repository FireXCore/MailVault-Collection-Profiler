use std::{collections::BTreeMap, str::FromStr};

use profiler_core::{
    ErrorCode, FindingDetail, FindingReviewEvent, FindingReviewHistory, ProfilerError,
    ProfilerResult, ReviewAction, ReviewActorKind, ReviewStatus, ReviewSummary,
    SanitizedFindingRow, SanitizedFormatSummary, SanitizedRunSummary, normalize_review_note,
    redaction_token, validate_status_note,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use super::{ProfilerStore, now_text, sqlite_error, to_u64};

struct ReviewEventRequest<'a> {
    run_id: &'a str,
    finding_id: &'a str,
    action: ReviewAction,
    requested_status: Option<ReviewStatus>,
    note: Option<&'a str>,
    actor_kind: ReviewActorKind,
    actor_label: Option<&'a str>,
}

impl ProfilerStore {
    pub fn set_finding_review_status(
        &mut self,
        run_id: &str,
        finding_id: &str,
        status: ReviewStatus,
        note: Option<&str>,
        actor_kind: ReviewActorKind,
        actor_label: Option<&str>,
    ) -> ProfilerResult<FindingReviewHistory> {
        let note = validate_status_note(status, note)?;
        self.append_review_event(&ReviewEventRequest {
            run_id,
            finding_id,
            action: ReviewAction::StatusSet,
            requested_status: Some(status),
            note: note.as_deref(),
            actor_kind,
            actor_label,
        })?;
        self.finding_review_history(run_id, finding_id)
    }

    pub fn clear_finding_review_status(
        &mut self,
        run_id: &str,
        finding_id: &str,
        note: Option<&str>,
        actor_kind: ReviewActorKind,
        actor_label: Option<&str>,
    ) -> ProfilerResult<FindingReviewHistory> {
        let note = normalize_review_note(note)?;
        self.append_review_event(&ReviewEventRequest {
            run_id,
            finding_id,
            action: ReviewAction::StatusCleared,
            requested_status: None,
            note: note.as_deref(),
            actor_kind,
            actor_label,
        })?;
        self.finding_review_history(run_id, finding_id)
    }

    pub fn add_finding_review_note(
        &mut self,
        run_id: &str,
        finding_id: &str,
        note: &str,
        actor_kind: ReviewActorKind,
        actor_label: Option<&str>,
    ) -> ProfilerResult<FindingReviewHistory> {
        let note = normalize_review_note(Some(note))?.ok_or_else(|| {
            ProfilerError::contract(
                ErrorCode::ReviewNoteRequired,
                "a non-empty review note is required",
                false,
            )
        })?;
        self.append_review_event(&ReviewEventRequest {
            run_id,
            finding_id,
            action: ReviewAction::NoteAdded,
            requested_status: None,
            note: Some(&note),
            actor_kind,
            actor_label,
        })?;
        self.finding_review_history(run_id, finding_id)
    }

    pub fn finding_review_history(
        &self,
        run_id: &str,
        finding_id: &str,
    ) -> ProfilerResult<FindingReviewHistory> {
        ensure_finding_exists(&self.connection, run_id, finding_id)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT event_id, run_id, finding_id, sequence, action, previous_status,
                        new_status, note, actor_kind, actor_label, occurred_at,
                        previous_event_hash, event_hash
                 FROM finding_review_events
                 WHERE run_id=?1 AND finding_id=?2
                 ORDER BY sequence",
            )
            .map_err(|source| sqlite_error("preparing finding review history", source))?;
        let events = statement
            .query_map((run_id, finding_id), review_event_from_row)
            .map_err(|source| sqlite_error("querying finding review history", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting finding review history", source))?;
        let state = self
            .connection
            .query_row(
                "SELECT current_status, latest_note FROM finding_review_state
                 WHERE run_id=?1 AND finding_id=?2",
                (run_id, finding_id),
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|source| sqlite_error("reading finding review state", source))?;
        let current_status = state
            .as_ref()
            .and_then(|value| value.0.as_deref())
            .map(ReviewStatus::from_str)
            .transpose()?;
        let integrity_valid = validate_event_sequence(&events).is_ok()
            && projection_matches(
                &events,
                current_status,
                state.as_ref().and_then(|value| value.1.as_deref()),
            );
        Ok(FindingReviewHistory {
            finding_id: finding_id.to_owned(),
            current_status,
            latest_note: state.and_then(|value| value.1),
            integrity_valid,
            events,
        })
    }

    pub fn finding_detail(&self, run_id: &str, finding_id: &str) -> ProfilerResult<FindingDetail> {
        let finding = self.finding_by_id(run_id, finding_id)?;
        let object = match finding.content_object_id.as_deref() {
            Some(content_object_id) => {
                let collection_id: String = self
                    .connection
                    .query_row(
                        "SELECT collection_id FROM profiler_runs WHERE id=?1",
                        [run_id],
                        |row| row.get(0),
                    )
                    .map_err(|source| sqlite_error("reading finding collection", source))?;
                Some(self.content_object_detail(run_id, &collection_id, content_object_id)?)
            }
            None => None,
        };
        let review = self.finding_review_history(run_id, finding_id)?;
        Ok(FindingDetail {
            finding,
            object,
            review,
        })
    }

    pub fn validate_all_review_history(&self) -> ProfilerResult<()> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT event_id, run_id, finding_id, sequence, action, previous_status,
                        new_status, note, actor_kind, actor_label, occurred_at,
                        previous_event_hash, event_hash
                 FROM finding_review_events
                 ORDER BY run_id, finding_id, sequence",
            )
            .map_err(|source| sqlite_error("preparing review integrity validation", source))?;
        let events = statement
            .query_map([], review_event_from_row)
            .map_err(|source| sqlite_error("querying review integrity validation", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting review integrity validation", source))?;

        let mut grouped: BTreeMap<(String, String), Vec<FindingReviewEvent>> = BTreeMap::new();
        for event in events {
            grouped
                .entry((event.run_id.clone(), event.finding_id.clone()))
                .or_default()
                .push(event);
        }
        let projection_count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM finding_review_state", [], |row| {
                row.get(0)
            })
            .map_err(|source| sqlite_error("counting review projections", source))?;
        let projection_count =
            u64::try_from(projection_count).map_err(|_| review_integrity_error())?;
        let history_count = u64::try_from(grouped.len()).map_err(|_| review_integrity_error())?;
        if projection_count != history_count {
            return Err(review_integrity_error());
        }

        for ((run_id, finding_id), history) in grouped {
            validate_event_sequence(&history)?;
            let state = self
                .connection
                .query_row(
                    "SELECT current_status, latest_note, last_event_id, last_sequence
                     FROM finding_review_state WHERE run_id=?1 AND finding_id=?2",
                    (&run_id, &finding_id),
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(|source| sqlite_error("validating review projection", source))?
                .ok_or_else(review_integrity_error)?;
            let status = state.0.as_deref().map(ReviewStatus::from_str).transpose()?;
            let last = history.last().ok_or_else(review_integrity_error)?;
            if !projection_matches(&history, status, state.1.as_deref())
                || state.2 != last.event_id
                || u64::try_from(state.3).ok() != Some(last.sequence)
            {
                return Err(review_integrity_error());
            }
        }
        Ok(())
    }

    pub fn review_summary(&self, run_id: &str) -> ProfilerResult<ReviewSummary> {
        let values = self
            .connection
            .query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(finding.severity IN ('error','warning')), 0),
                    COALESCE(SUM(state.current_status IS NULL), 0),
                    COALESCE(SUM(state.current_status='acknowledged'), 0),
                    COALESCE(SUM(state.current_status='expected'), 0),
                    COALESCE(SUM(state.current_status='needs_investigation'), 0),
                    COALESCE(SUM(state.current_status='resolved_externally'), 0),
                    COALESCE(SUM(state.current_status IS NOT NULL), 0),
                    COALESCE(SUM(
                        finding.severity='warning' AND (
                            state.current_status IS NULL OR
                            state.current_status IN ('acknowledged','needs_investigation')
                        )
                    ), 0),
                    COALESCE(SUM(
                        finding.severity='error' AND (
                            state.current_status IS NULL OR
                            state.current_status IN ('acknowledged','needs_investigation')
                        )
                    ), 0),
                    COALESCE(SUM(finding.severity='info'), 0)
                 FROM findings AS finding
                 LEFT JOIN finding_review_state AS state
                   ON state.run_id=finding.run_id AND state.finding_id=finding.id
                 WHERE finding.run_id=?1 AND finding.resolved_at IS NULL",
                [run_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .map_err(|source| sqlite_error("reading review summary", source))?;
        let reviewable = to_u64(values.1, "reviewable finding count")?;
        let reviewed_reviewable: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM findings AS finding
                 JOIN finding_review_state AS state
                   ON state.run_id=finding.run_id AND state.finding_id=finding.id
                 WHERE finding.run_id=?1 AND finding.resolved_at IS NULL
                   AND finding.severity IN ('error','warning')
                   AND state.current_status IS NOT NULL",
                [run_id],
                |row| row.get(0),
            )
            .map_err(|source| sqlite_error("reading reviewed finding count", source))?;
        let reviewed_reviewable = to_u64(reviewed_reviewable, "reviewed reviewable count")?;
        let review_completion_percent = u32::try_from(
            reviewed_reviewable
                .saturating_mul(100)
                .checked_div(reviewable)
                .unwrap_or(100),
        )
        .unwrap_or(100);
        Ok(ReviewSummary {
            total_findings: to_u64(values.0, "total finding count")?,
            reviewable_findings: reviewable,
            unreviewed: to_u64(values.2, "unreviewed finding count")?,
            acknowledged: to_u64(values.3, "acknowledged finding count")?,
            expected: to_u64(values.4, "expected finding count")?,
            needs_investigation: to_u64(values.5, "investigation finding count")?,
            resolved_externally: to_u64(values.6, "externally resolved finding count")?,
            reviewed_findings: to_u64(values.7, "reviewed finding count")?,
            review_completion_percent,
            warnings_remaining: to_u64(values.8, "remaining warning count")?,
            errors_remaining: to_u64(values.9, "remaining error count")?,
            informational_evidence: to_u64(values.10, "informational finding count")?,
        })
    }

    pub fn sanitized_run_summary(&self, run_id: &str) -> ProfilerResult<SanitizedRunSummary> {
        let run = self.run_catalog_entry(run_id)?;
        let collection_id = run.collection_id.as_deref().ok_or_else(|| {
            ProfilerError::contract(ErrorCode::RunNotBrowsable, "run has no collection", false)
        })?;
        let snapshot_id = run.source_snapshot_id.as_deref().ok_or_else(|| {
            ProfilerError::contract(
                ErrorCode::RunNotBrowsable,
                "run has no source snapshot",
                false,
            )
        })?;
        let inventory = self.run_inventory_summary(collection_id, snapshot_id)?;
        let findings = self.findings_summary(run_id)?;
        let review = self.review_summary(run_id)?;
        let format_summary = self.format_summary(run_id)?;
        let exact_formats = SanitizedFormatSummary {
            latest_format_run_id: format_summary.latest_format_run_id,
            latest_run_state: format_summary.latest_run_state,
            total_objects: format_summary.total_objects,
            eligible_objects: format_summary.eligible_objects,
            completed_objects: format_summary.completed_objects,
            total_bytes: format_summary.total_bytes,
            completed_bytes: format_summary.completed_bytes,
            identified: format_summary.identified,
            unknown: format_summary.unknown,
            ambiguous: format_summary.ambiguous,
            empty: format_summary.empty,
            skipped_unavailable: format_summary.skipped_unavailable,
            tool_errors: format_summary.tool_errors,
            extension_mismatches: format_summary.extension_mismatches,
            distinct_puids: format_summary.distinct_puids,
            tool_name: format_summary
                .tool
                .as_ref()
                .map(|tool| tool.tool_name.clone()),
            tool_version: format_summary
                .tool
                .as_ref()
                .map(|tool| tool.tool_version.clone()),
            executable_sha256: format_summary
                .tool
                .as_ref()
                .map(|tool| tool.executable_sha256.clone()),
            signature_version: format_summary
                .tool
                .as_ref()
                .map(|tool| tool.signature_version.clone()),
            signature_sha256: format_summary
                .tool
                .as_ref()
                .and_then(|tool| tool.signature_sha256.clone()),
            started_at: format_summary.started_at,
            finished_at: format_summary.finished_at,
        };
        let findings_by_code = map_counts(
            &self.connection,
            "SELECT code, COUNT(*) FROM findings
             WHERE run_id=?1 AND resolved_at IS NULL GROUP BY code ORDER BY code",
            run_id,
        )?;
        let review_by_status = map_counts(
            &self.connection,
            "SELECT COALESCE(current_status, 'unreviewed'), COUNT(*)
             FROM findings AS finding
             LEFT JOIN finding_review_state AS state
               ON state.run_id=finding.run_id AND state.finding_id=finding.id
             WHERE finding.run_id=?1 AND finding.resolved_at IS NULL
             GROUP BY COALESCE(current_status, 'unreviewed')
             ORDER BY COALESCE(current_status, 'unreviewed')",
            run_id,
        )?;
        Ok(SanitizedRunSummary {
            format_version: 2,
            generated_at: now_text(),
            run_id: run_id.to_owned(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            workspace_schema_version: super::CURRENT_USER_VERSION,
            run_status: run.status,
            source_mutation: "none".into(),
            inventory,
            findings,
            review,
            exact_formats,
            findings_by_code,
            review_by_status,
        })
    }

    pub fn sanitized_findings(&self, run_id: &str) -> ProfilerResult<Vec<SanitizedFindingRow>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT finding.id, finding.code, finding.severity, state.current_status,
                        state.reviewed_at, content.canonical_path_display
                 FROM findings AS finding
                 LEFT JOIN finding_review_state AS state
                   ON state.run_id=finding.run_id AND state.finding_id=finding.id
                 LEFT JOIN content_objects AS content ON content.id=finding.content_object_id
                 WHERE finding.run_id=?1 AND finding.resolved_at IS NULL
                 ORDER BY finding.id",
            )
            .map_err(|source| sqlite_error("preparing sanitized findings export", source))?;
        statement
            .query_map([run_id], |row| {
                let review_status = row
                    .get::<_, Option<String>>(3)?
                    .as_deref()
                    .map(ReviewStatus::from_str)
                    .transpose()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                let locator = row.get::<_, Option<String>>(5)?;
                let finding_id = row.get::<_, String>(0)?;
                Ok(SanitizedFindingRow {
                    finding_token: redaction_token(&finding_id),
                    object_token: locator.as_deref().map(redaction_token),
                    code: row.get(1)?,
                    severity: row.get(2)?,
                    review_status,
                    reviewed_at: row.get(4)?,
                })
            })
            .map_err(|source| sqlite_error("querying sanitized findings export", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting sanitized findings export", source))
    }

    fn append_review_event(&mut self, request: &ReviewEventRequest<'_>) -> ProfilerResult<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| sqlite_error("starting review transaction", source))?;
        ensure_finding_exists(&transaction, request.run_id, request.finding_id)?;

        let event = prepare_review_event(&transaction, request)?;
        insert_review_event(&transaction, &event)?;
        update_review_projection(&transaction, &event)?;

        transaction
            .commit()
            .map_err(|source| sqlite_error("committing finding review event", source))
    }
}

fn prepare_review_event(
    transaction: &Transaction<'_>,
    request: &ReviewEventRequest<'_>,
) -> ProfilerResult<FindingReviewEvent> {
    let previous = load_projection(transaction, request.run_id, request.finding_id)?;
    let previous_status = previous
        .as_ref()
        .and_then(|value| value.current_status.as_deref())
        .map(ReviewStatus::from_str)
        .transpose()?;
    let sequence = next_review_sequence(previous.as_ref())?;
    let new_status = next_review_status(request.action, request.requested_status, previous_status)?;
    let occurred_at = now_text();
    let mut event = FindingReviewEvent {
        event_id: Uuid::now_v7().to_string(),
        run_id: request.run_id.to_owned(),
        finding_id: request.finding_id.to_owned(),
        sequence,
        action: request.action,
        previous_status,
        new_status,
        note: request.note.map(str::to_owned),
        actor_kind: request.actor_kind,
        actor_label: request.actor_label.map(str::to_owned),
        occurred_at,
        previous_event_hash: previous.map(|value| value.last_event_hash),
        event_hash: String::new(),
    };
    event.event_hash = event.compute_hash()?;
    Ok(event)
}

fn next_review_sequence(previous: Option<&ProjectionRow>) -> ProfilerResult<u64> {
    previous.map_or(Ok(1), |value| {
        value
            .last_sequence
            .checked_add(1)
            .ok_or_else(|| ProfilerError::Internal("review sequence exhausted u64 capacity".into()))
    })
}

fn next_review_status(
    action: ReviewAction,
    requested_status: Option<ReviewStatus>,
    previous_status: Option<ReviewStatus>,
) -> ProfilerResult<Option<ReviewStatus>> {
    match action {
        ReviewAction::StatusSet => requested_status
            .ok_or_else(|| {
                ProfilerError::Internal(
                    "status-set review event is missing the requested status".into(),
                )
            })
            .map(Some),
        ReviewAction::StatusCleared => Ok(None),
        ReviewAction::NoteAdded => Ok(previous_status),
    }
}

fn insert_review_event(
    transaction: &Transaction<'_>,
    event: &FindingReviewEvent,
) -> ProfilerResult<()> {
    transaction
        .execute(
            "INSERT INTO finding_review_events(
                event_id, run_id, finding_id, sequence, action, previous_status, new_status,
                note, actor_kind, actor_label, occurred_at, previous_event_hash, event_hash
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                event.event_id.as_str(),
                event.run_id.as_str(),
                event.finding_id.as_str(),
                review_sequence_to_i64(event.sequence)?,
                event.action.as_str(),
                event.previous_status.map(ReviewStatus::as_str),
                event.new_status.map(ReviewStatus::as_str),
                event.note.as_deref(),
                event.actor_kind.as_str(),
                event.actor_label.as_deref(),
                event.occurred_at.as_str(),
                event.previous_event_hash.as_deref(),
                event.event_hash.as_str(),
            ],
        )
        .map_err(|source| sqlite_error("appending finding review event", source))?;
    Ok(())
}

fn update_review_projection(
    transaction: &Transaction<'_>,
    event: &FindingReviewEvent,
) -> ProfilerResult<()> {
    transaction
        .execute(
            "INSERT INTO finding_review_state(
                run_id, finding_id, current_status, latest_note, last_event_id,
                last_sequence, reviewed_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(run_id, finding_id) DO UPDATE SET
                current_status=excluded.current_status,
                latest_note=COALESCE(excluded.latest_note, finding_review_state.latest_note),
                last_event_id=excluded.last_event_id,
                last_sequence=excluded.last_sequence,
                reviewed_at=excluded.reviewed_at",
            params![
                event.run_id.as_str(),
                event.finding_id.as_str(),
                event.new_status.map(ReviewStatus::as_str),
                event.note.as_deref(),
                event.event_id.as_str(),
                review_sequence_to_i64(event.sequence)?,
                event.occurred_at.as_str(),
            ],
        )
        .map_err(|source| sqlite_error("updating finding review projection", source))?;
    Ok(())
}

fn review_sequence_to_i64(sequence: u64) -> ProfilerResult<i64> {
    i64::try_from(sequence)
        .map_err(|_| ProfilerError::Internal("review sequence exceeds SQLite capacity".into()))
}

#[derive(Debug)]
struct ProjectionRow {
    current_status: Option<String>,
    last_sequence: u64,
    last_event_hash: String,
}

fn load_projection(
    transaction: &Transaction<'_>,
    run_id: &str,
    finding_id: &str,
) -> ProfilerResult<Option<ProjectionRow>> {
    transaction
        .query_row(
            "SELECT state.current_status, state.last_sequence, event.event_hash
             FROM finding_review_state AS state
             JOIN finding_review_events AS event ON event.event_id=state.last_event_id
             WHERE state.run_id=?1 AND state.finding_id=?2",
            (run_id, finding_id),
            |row| {
                let sequence = row.get::<_, i64>(1)?;
                let sequence = u64::try_from(sequence).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?;
                Ok(ProjectionRow {
                    current_status: row.get(0)?,
                    last_sequence: sequence,
                    last_event_hash: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|source| sqlite_error("loading finding review projection", source))
}

fn ensure_finding_exists(
    connection: &rusqlite::Connection,
    run_id: &str,
    finding_id: &str,
) -> ProfilerResult<()> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM findings WHERE run_id=?1 AND id=?2 AND resolved_at IS NULL)",
            (run_id, finding_id),
            |row| row.get(0),
        )
        .map_err(|source| sqlite_error("validating finding identity", source))?;
    if exists {
        Ok(())
    } else {
        Err(ProfilerError::contract(
            ErrorCode::FindingNotFound,
            "finding was not found in the selected run",
            false,
        ))
    }
}

fn review_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FindingReviewEvent> {
    let sequence = row.get::<_, i64>(3)?;
    let action = ReviewAction::from_str(&row.get::<_, String>(4)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let previous_status = row
        .get::<_, Option<String>>(5)?
        .as_deref()
        .map(ReviewStatus::from_str)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let new_status = row
        .get::<_, Option<String>>(6)?
        .as_deref()
        .map(ReviewStatus::from_str)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let actor_kind = ReviewActorKind::from_str(&row.get::<_, String>(8)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let sequence = u64::try_from(sequence).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    Ok(FindingReviewEvent {
        event_id: row.get(0)?,
        run_id: row.get(1)?,
        finding_id: row.get(2)?,
        sequence,
        action,
        previous_status,
        new_status,
        note: row.get(7)?,
        actor_kind,
        actor_label: row.get(9)?,
        occurred_at: row.get(10)?,
        previous_event_hash: row.get(11)?,
        event_hash: row.get(12)?,
    })
}

fn validate_event_sequence(events: &[FindingReviewEvent]) -> ProfilerResult<()> {
    let mut previous_hash: Option<&str> = None;
    let mut current_status: Option<ReviewStatus> = None;
    let mut identity: Option<(&str, &str)> = None;

    for (index, event) in events.iter().enumerate() {
        let expected_sequence = u64::try_from(index + 1).map_err(|_| review_integrity_error())?;
        let event_identity = (event.run_id.as_str(), event.finding_id.as_str());
        if identity.is_some_and(|expected| expected != event_identity)
            || event.sequence != expected_sequence
            || event.previous_event_hash.as_deref() != previous_hash
            || event.previous_status != current_status
            || !event_transition_is_valid(event, current_status)
            || event.compute_hash()? != event.event_hash
        {
            return Err(review_integrity_error());
        }

        identity = Some(event_identity);
        current_status = event.new_status;
        previous_hash = Some(event.event_hash.as_str());
    }
    Ok(())
}

fn event_transition_is_valid(
    event: &FindingReviewEvent,
    current_status: Option<ReviewStatus>,
) -> bool {
    match event.action {
        ReviewAction::StatusSet => event.new_status.is_some(),
        ReviewAction::StatusCleared => event.new_status.is_none(),
        ReviewAction::NoteAdded => event.new_status == current_status && event.note.is_some(),
    }
}

fn projection_matches(
    events: &[FindingReviewEvent],
    current_status: Option<ReviewStatus>,
    latest_note: Option<&str>,
) -> bool {
    let Some(last) = events.last() else {
        return current_status.is_none() && latest_note.is_none();
    };
    let expected_status = last.new_status;
    let expected_note = events.iter().rev().find_map(|event| event.note.as_deref());
    expected_status == current_status && expected_note == latest_note
}

fn review_integrity_error() -> ProfilerError {
    ProfilerError::contract(
        ErrorCode::ReviewHistoryIntegrityFailure,
        "finding review history failed integrity validation",
        false,
    )
}

fn map_counts(
    connection: &rusqlite::Connection,
    sql: &str,
    run_id: &str,
) -> ProfilerResult<BTreeMap<String, u64>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|source| sqlite_error("preparing sanitized count map", source))?;
    let values = statement
        .query_map([run_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|source| sqlite_error("querying sanitized count map", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| sqlite_error("collecting sanitized count map", source))?;
    values
        .into_iter()
        .map(|(key, value)| Ok((key, to_u64(value, "sanitized count")?)))
        .collect()
}

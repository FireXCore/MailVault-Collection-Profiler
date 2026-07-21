use std::str::FromStr;

use profiler_core::{
    AvailabilityState, ContentObjectDetail, FilenameVariantView, FindingCategory, FindingView,
    FindingsPage, FindingsPageRequest, FindingsSummary, InventoryObjectRow, InventoryPage,
    InventoryPageRequest, OccurrenceView, ProfilerError, ProfilerResult, ReviewStatus, SizeState,
};
use rusqlite::{OptionalExtension, named_params, types::Type};

use crate::{ProfilerStore, sqlite_error, to_u64};

const MAX_PAGE_SIZE: u32 = 250;
const MAX_DETAIL_OCCURRENCES: u32 = 500;

impl ProfilerStore {
    pub fn inventory_page(&self, request: &InventoryPageRequest) -> ProfilerResult<InventoryPage> {
        validate_inventory_page_request(request)?;
        let search = normalized_search(request.filters.search.as_deref());
        let search_like = search.as_deref().map(|value| format!("%{value}%"));
        let after_sha256 = request.after_sha256.as_deref().map(str::to_ascii_lowercase);
        let availability = request
            .filters
            .availability_state
            .map(AvailabilityState::as_str);
        let size_state = request.filters.size_state.map(SizeState::as_str);
        let finding_code = normalized_finding_code(request.filters.finding_code.as_deref())?;
        let fetch_limit = i64::from(request.limit) + 1;

        let total_filtered = self
            .connection
            .query_row(
                inventory_count_sql(),
                named_params! {
                    ":collection_id": request.collection_id.as_str(),
                    ":run_id": request.run_id.as_str(),
                    ":search_like": search_like.as_deref(),
                    ":availability": availability,
                    ":size_state": size_state,
                    ":finding_code": finding_code.as_deref(),
                },
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| sqlite_error("counting filtered inventory objects", source))?;

        let mut statement = self
            .connection
            .prepare_cached(inventory_page_sql())
            .map_err(|source| sqlite_error("preparing inventory page", source))?;
        let mut items = statement
            .query_map(
                named_params! {
                    ":collection_id": request.collection_id.as_str(),
                    ":run_id": request.run_id.as_str(),
                    ":after_sha256": after_sha256.as_deref(),
                    ":search_like": search_like.as_deref(),
                    ":availability": availability,
                    ":size_state": size_state,
                    ":finding_code": finding_code.as_deref(),
                    ":fetch_limit": fetch_limit,
                },
                inventory_object_from_row,
            )
            .map_err(|source| sqlite_error("querying inventory page", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting inventory page", source))?;

        let has_more = items.len() > request.limit as usize;
        if has_more {
            items.truncate(request.limit as usize);
        }
        let next_after_sha256 = if has_more {
            items.last().map(|item| item.sha256.clone())
        } else {
            None
        };

        Ok(InventoryPage {
            items,
            total_filtered: to_u64(total_filtered, "filtered inventory count")?,
            next_after_sha256,
            has_more,
        })
    }

    pub fn content_object_detail(
        &self,
        run_id: &str,
        collection_id: &str,
        content_object_id: &str,
    ) -> ProfilerResult<ContentObjectDetail> {
        if run_id.trim().is_empty()
            || collection_id.trim().is_empty()
            || content_object_id.trim().is_empty()
        {
            return Err(ProfilerError::InvalidArgument(
                "run id, collection id and content object id are required".into(),
            ));
        }

        let object = self
            .connection
            .query_row(
                inventory_object_by_id_sql(),
                named_params! {
                    ":run_id": run_id,
                    ":collection_id": collection_id,
                    ":content_object_id": content_object_id,
                },
                inventory_object_from_row,
            )
            .optional()
            .map_err(|source| sqlite_error("loading content object", source))?
            .ok_or_else(|| {
                ProfilerError::InvalidArgument(format!(
                    "content object does not exist: {content_object_id}"
                ))
            })?;

        let filename_variants = self.filename_variants(content_object_id)?;
        let occurrence_total = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM content_occurrences WHERE content_object_id=?1",
                [content_object_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| sqlite_error("counting content occurrences", source))?;
        let occurrences = self.occurrences(content_object_id, MAX_DETAIL_OCCURRENCES)?;
        let findings = self.findings_for_object(run_id, content_object_id)?;
        let occurrence_total = to_u64(occurrence_total, "content occurrence count")?;

        Ok(ContentObjectDetail {
            object,
            filename_variants,
            occurrences_truncated: occurrence_total
                > u64::try_from(occurrences.len()).unwrap_or(u64::MAX),
            occurrence_total,
            occurrences,
            findings,
        })
    }

    pub fn findings_page(&self, request: &FindingsPageRequest) -> ProfilerResult<FindingsPage> {
        validate_findings_page_request(request)?;
        let code = normalized_finding_code(request.code.as_deref())?;
        let severity = normalized_severity(request.severity.as_deref())?;
        let review_status = normalized_review_status_filter(request.review_status.as_deref())?;
        let category = request.category.as_ref().map(category_value);
        let search = normalized_search(request.search.as_deref());
        let search_like = search.as_deref().map(|value| format!("%{value}%"));
        let fetch_limit = i64::from(request.limit) + 1;

        let summary = self.findings_summary(&request.run_id)?;
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT finding.id, finding.content_object_id, finding.code, finding.severity,
                        finding.message, finding.evidence_json, finding.created_at,
                        state.current_status, state.reviewed_at
                 FROM findings AS finding
                 LEFT JOIN finding_review_state AS state
                   ON state.run_id=finding.run_id AND state.finding_id=finding.id
                 WHERE finding.run_id=:run_id AND finding.resolved_at IS NULL
                   AND (:code IS NULL OR finding.code=:code)
                   AND (:severity IS NULL OR finding.severity=:severity)
                   AND (:review_status IS NULL
                        OR (:review_status='unreviewed' AND state.current_status IS NULL)
                        OR state.current_status=:review_status)
                   AND (:category IS NULL OR :category='all'
                        OR (:category='requires_attention' AND finding.severity IN ('error','warning'))
                        OR (:category='informational_evidence' AND finding.severity='info')
                        OR (:category='reviewed' AND state.current_status IS NOT NULL))
                   AND (:search_like IS NULL OR lower(finding.code) LIKE :search_like
                        OR lower(finding.message) LIKE :search_like)
                   AND (:after_id IS NULL OR finding.id > :after_id)
                 ORDER BY finding.id LIMIT :fetch_limit",
            )
            .map_err(|source| sqlite_error("preparing findings page", source))?;
        let mut items = statement
            .query_map(
                named_params! {
                    ":run_id": request.run_id.as_str(),
                    ":code": code.as_deref(),
                    ":severity": severity.as_deref(),
                    ":review_status": review_status.as_deref(),
                    ":category": category,
                    ":search_like": search_like.as_deref(),
                    ":after_id": request.after_id.as_deref(),
                    ":fetch_limit": fetch_limit,
                },
                finding_from_row,
            )
            .map_err(|source| sqlite_error("querying findings page", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting findings page", source))?;

        let has_more = items.len() > request.limit as usize;
        if has_more {
            items.truncate(request.limit as usize);
        }
        let next_after_id = if has_more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };

        Ok(FindingsPage {
            items,
            summary,
            next_after_id,
            has_more,
        })
    }

    pub fn finding_by_id(&self, run_id: &str, finding_id: &str) -> ProfilerResult<FindingView> {
        self.connection
            .query_row(
                "SELECT finding.id, finding.content_object_id, finding.code, finding.severity,
                        finding.message, finding.evidence_json, finding.created_at,
                        state.current_status, state.reviewed_at
                 FROM findings AS finding
                 LEFT JOIN finding_review_state AS state
                   ON state.run_id=finding.run_id AND state.finding_id=finding.id
                 WHERE finding.run_id=?1 AND finding.id=?2 AND finding.resolved_at IS NULL",
                (run_id, finding_id),
                finding_from_row,
            )
            .optional()
            .map_err(|source| sqlite_error("loading finding detail", source))?
            .ok_or_else(|| {
                ProfilerError::contract(
                    profiler_core::ErrorCode::FindingNotFound,
                    "finding was not found in the selected run",
                    false,
                )
            })
    }

    fn filename_variants(
        &self,
        content_object_id: &str,
    ) -> ProfilerResult<Vec<FilenameVariantView>> {
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT normalized_filename, display_filename, occurrence_count, \
                        first_seen_at, last_seen_at \
                 FROM filename_variants WHERE content_object_id=?1 \
                 ORDER BY occurrence_count DESC, normalized_filename",
            )
            .map_err(|source| sqlite_error("preparing filename variants", source))?;
        statement
            .query_map([content_object_id], |row| {
                Ok(FilenameVariantView {
                    normalized_filename: row.get(0)?,
                    display_filename: row.get(1)?,
                    occurrence_count: row_u64(row, 2)?,
                    first_seen_at: row.get(3)?,
                    last_seen_at: row.get(4)?,
                })
            })
            .map_err(|source| sqlite_error("querying filename variants", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting filename variants", source))
    }

    fn occurrences(
        &self,
        content_object_id: &str,
        limit: u32,
    ) -> ProfilerResult<Vec<OccurrenceView>> {
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT occurrence.id, occurrence.source_message_id, occurrence.source_part_id, \
                        occurrence.part_path, occurrence.filename_original, occurrence.role, \
                        occurrence.sender_domain, occurrence.message_date, message.subject_raw, \
                        message.provider_thread_namespace, message.provider_thread_value \
                 FROM content_occurrences AS occurrence \
                 JOIN source_messages AS message \
                   ON message.snapshot_id=occurrence.snapshot_id \
                  AND message.source_message_id=occurrence.source_message_id \
                 WHERE occurrence.content_object_id=?1 \
                 ORDER BY COALESCE(occurrence.message_date, ''), occurrence.source_message_id, \
                          occurrence.part_path \
                 LIMIT ?2",
            )
            .map_err(|source| sqlite_error("preparing content occurrences", source))?;
        statement
            .query_map((content_object_id, i64::from(limit)), |row| {
                Ok(OccurrenceView {
                    occurrence_id: row.get(0)?,
                    source_message_id: row.get(1)?,
                    source_part_id: row.get(2)?,
                    part_path: row.get(3)?,
                    filename_original: row.get(4)?,
                    role: row.get(5)?,
                    sender_domain: row.get(6)?,
                    message_date: row.get(7)?,
                    subject: row.get(8)?,
                    provider_thread_namespace: row.get(9)?,
                    provider_thread_value: row.get(10)?,
                })
            })
            .map_err(|source| sqlite_error("querying content occurrences", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting content occurrences", source))
    }

    fn findings_for_object(
        &self,
        run_id: &str,
        content_object_id: &str,
    ) -> ProfilerResult<Vec<FindingView>> {
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT finding.id, finding.content_object_id, finding.code, finding.severity,
                        finding.message, finding.evidence_json, finding.created_at,
                        state.current_status, state.reviewed_at
                 FROM findings AS finding
                 LEFT JOIN finding_review_state AS state
                   ON state.run_id=finding.run_id AND state.finding_id=finding.id
                 WHERE finding.run_id=?1 AND finding.content_object_id=?2
                   AND finding.resolved_at IS NULL
                 ORDER BY CASE finding.severity WHEN 'error' THEN 0 WHEN 'warning' THEN 1 ELSE 2 END,
                          finding.code",
            )
            .map_err(|source| sqlite_error("preparing object findings", source))?;
        statement
            .query_map((run_id, content_object_id), finding_from_row)
            .map_err(|source| sqlite_error("querying object findings", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| sqlite_error("collecting object findings", source))
    }

    pub fn findings_summary(&self, run_id: &str) -> ProfilerResult<FindingsSummary> {
        let values = self
            .connection
            .query_row(
                "SELECT COUNT(*), \
                        COALESCE(SUM(severity='warning'), 0), \
                        COALESCE(SUM(severity='error'), 0), \
                        COALESCE(SUM(severity='info'), 0), \
                        COALESCE(SUM(code='ZERO_BYTE_CONTENT'), 0), \
                        COALESCE(SUM(code='SAME_HASH_DIFFERENT_NAMES'), 0), \
                        COALESCE(SUM(code='SAME_NAME_DIFFERENT_HASHES'), 0), \
                        COALESCE(SUM(code='MISSING_BLOB'), 0), \
                        COALESCE(SUM(code='BLOB_SIZE_MISMATCH'), 0), \
                        COALESCE(SUM(code='INVALID_BLOB_LOCATOR'), 0) \
                 FROM findings WHERE run_id=?1 AND resolved_at IS NULL",
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
                    ))
                },
            )
            .map_err(|source| sqlite_error("reading findings summary", source))?;
        Ok(FindingsSummary {
            total: to_u64(values.0, "finding total")?,
            warnings: to_u64(values.1, "finding warning count")?,
            errors: to_u64(values.2, "finding error count")?,
            informational: to_u64(values.3, "finding info count")?,
            zero_byte: to_u64(values.4, "zero-byte finding count")?,
            same_hash_different_names: to_u64(values.5, "filename variant finding count")?,
            same_name_different_hashes: to_u64(values.6, "filename collision finding count")?,
            missing: to_u64(values.7, "missing finding count")?,
            size_mismatch: to_u64(values.8, "size mismatch finding count")?,
            invalid_locator: to_u64(values.9, "invalid locator finding count")?,
        })
    }
}

fn validate_inventory_page_request(request: &InventoryPageRequest) -> ProfilerResult<()> {
    if request.collection_id.trim().is_empty() || request.run_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument(
            "collection id and run id are required".into(),
        ));
    }
    validate_page_size(request.limit)?;
    if let Some(after) = request.after_sha256.as_deref()
        && (after.len() != 64 || !after.bytes().all(|value| value.is_ascii_hexdigit()))
    {
        return Err(ProfilerError::InvalidArgument(
            "inventory cursor is not a SHA-256 value".into(),
        ));
    }
    Ok(())
}

fn validate_findings_page_request(request: &FindingsPageRequest) -> ProfilerResult<()> {
    if request.run_id.trim().is_empty() {
        return Err(ProfilerError::InvalidArgument("run id is required".into()));
    }
    validate_page_size(request.limit)
}

fn validate_page_size(limit: u32) -> ProfilerResult<()> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ProfilerError::InvalidArgument(format!(
            "page size must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn normalized_search(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

fn normalized_finding_code(value: Option<&str>) -> ProfilerResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_uppercase();
    if !normalized
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ProfilerError::InvalidArgument(
            "finding code may contain only A-Z, 0-9 and underscore".into(),
        ));
    }
    Ok(Some(normalized))
}

fn normalized_severity(value: Option<&str>) -> ProfilerResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if !matches!(normalized.as_str(), "info" | "warning" | "error") {
        return Err(ProfilerError::InvalidArgument(
            "finding severity must be info, warning or error".into(),
        ));
    }
    Ok(Some(normalized))
}

fn normalized_review_status_filter(value: Option<&str>) -> ProfilerResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if normalized == "unreviewed" {
        return Ok(Some(normalized));
    }
    ReviewStatus::from_str(&normalized)?;
    Ok(Some(normalized))
}

const fn category_value(category: &FindingCategory) -> &'static str {
    match category {
        FindingCategory::RequiresAttention => "requires_attention",
        FindingCategory::InformationalEvidence => "informational_evidence",
        FindingCategory::Reviewed => "reviewed",
        FindingCategory::All => "all",
    }
}

fn inventory_object_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<InventoryObjectRow> {
    let availability: String = row.get(12)?;
    let size_state: String = row.get(13)?;
    Ok(InventoryObjectRow {
        id: row.get(0)?,
        sha256: row.get(1)?,
        primary_filename: row.get(2)?,
        source_detected_mime_type: row.get(3)?,
        expected_size_bytes: row_u64(row, 4)?,
        actual_size_bytes: row_optional_u64(row, 5)?,
        occurrence_count: row_u64(row, 6)?,
        filename_variant_count: row_u64(row, 7)?,
        message_count: row_u64(row, 8)?,
        thread_count: row_u64(row, 9)?,
        first_seen_at: row.get(10)?,
        last_seen_at: row.get(11)?,
        availability_state: parse_availability(&availability, 12)?,
        size_state: parse_size_state(&size_state, 13)?,
        finding_count: row_u64(row, 14)?,
    })
}

pub(super) fn finding_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FindingView> {
    let evidence_json: String = row.get(5)?;
    let evidence = serde_json::from_str(&evidence_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, Type::Text, Box::new(error))
    })?;
    Ok(FindingView {
        id: row.get(0)?,
        content_object_id: row.get(1)?,
        code: row.get(2)?,
        severity: row.get(3)?,
        message: row.get(4)?,
        evidence,
        created_at: row.get(6)?,
        review_status: row
            .get::<_, Option<String>>(7)?
            .as_deref()
            .map(ReviewStatus::from_str)
            .transpose()
            .map_err(|error| invalid_database_value(7, Type::Text, error.to_string()))?,
        reviewed_at: row.get(8)?,
    })
}

fn parse_availability(value: &str, column: usize) -> rusqlite::Result<AvailabilityState> {
    match value {
        "uninspected" => Ok(AvailabilityState::Uninspected),
        "available" => Ok(AvailabilityState::Available),
        "missing" => Ok(AvailabilityState::Missing),
        "unreadable" => Ok(AvailabilityState::Unreadable),
        "invalid_locator" => Ok(AvailabilityState::InvalidLocator),
        "non_regular" => Ok(AvailabilityState::NonRegular),
        "unsafe_reparse_point" => Ok(AvailabilityState::UnsafeReparsePoint),
        "io_error" => Ok(AvailabilityState::IoError),
        other => Err(invalid_database_value(
            column,
            Type::Text,
            format!("unknown availability state: {other}"),
        )),
    }
}

fn parse_size_state(value: &str, column: usize) -> rusqlite::Result<SizeState> {
    match value {
        "uninspected" => Ok(SizeState::Uninspected),
        "match" => Ok(SizeState::Match),
        "mismatch" => Ok(SizeState::Mismatch),
        "unavailable" => Ok(SizeState::Unavailable),
        other => Err(invalid_database_value(
            column,
            Type::Text,
            format!("unknown size state: {other}"),
        )),
    }
}

fn row_u64(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(column)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
}

fn row_optional_u64(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(column)?
        .map(|value| {
            u64::try_from(value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
            })
        })
        .transpose()
}

fn invalid_database_value(column: usize, value_type: Type, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        value_type,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn inventory_count_sql() -> &'static str {
    "SELECT COUNT(*) FROM content_objects AS content \
     WHERE content.collection_id=:collection_id \
       AND (:availability IS NULL OR content.availability_state=:availability) \
       AND (:size_state IS NULL OR content.size_state=:size_state) \
       AND (:finding_code IS NULL OR EXISTS ( \
            SELECT 1 FROM findings AS finding \
            WHERE finding.run_id=:run_id AND finding.content_object_id=content.id \
              AND finding.code=:finding_code AND finding.resolved_at IS NULL \
       )) \
       AND (:search_like IS NULL OR lower(content.sha256) LIKE :search_like \
            OR lower(content.source_detected_mime_type) LIKE :search_like \
            OR EXISTS (SELECT 1 FROM filename_variants AS variant \
                       WHERE variant.content_object_id=content.id \
                         AND lower(variant.display_filename) LIKE :search_like) \
            OR EXISTS (SELECT 1 FROM content_occurrences AS occurrence \
                       JOIN source_messages AS message \
                         ON message.snapshot_id=occurrence.snapshot_id \
                        AND message.source_message_id=occurrence.source_message_id \
                       WHERE occurrence.content_object_id=content.id \
                         AND (lower(COALESCE(occurrence.sender_domain, '')) LIKE :search_like \
                              OR lower(message.subject_raw) LIKE :search_like)))"
}

fn inventory_page_sql() -> &'static str {
    "SELECT content.id, content.sha256, \
            COALESCE((SELECT variant.display_filename FROM filename_variants AS variant \
                      WHERE variant.content_object_id=content.id \
                      ORDER BY variant.occurrence_count DESC, variant.normalized_filename LIMIT 1), \
                     '[unnamed]'), \
            content.source_detected_mime_type, content.expected_size_bytes, \
            content.actual_size_bytes, content.occurrence_count, \
            (SELECT COUNT(*) FROM filename_variants AS variant \
             WHERE variant.content_object_id=content.id), \
            (SELECT COUNT(DISTINCT occurrence.source_message_id) \
             FROM content_occurrences AS occurrence \
             WHERE occurrence.content_object_id=content.id), \
            (SELECT COUNT(DISTINCT COALESCE(message.provider_thread_namespace, '') || ':' || \
                                   COALESCE(message.provider_thread_value, '')) \
             FROM content_occurrences AS occurrence \
             JOIN source_messages AS message \
               ON message.snapshot_id=occurrence.snapshot_id \
              AND message.source_message_id=occurrence.source_message_id \
             WHERE occurrence.content_object_id=content.id \
               AND message.provider_thread_value IS NOT NULL), \
            content.first_seen_at, content.last_seen_at, content.availability_state, \
            content.size_state, \
            (SELECT COUNT(*) FROM findings AS finding \
             WHERE finding.run_id=:run_id AND finding.content_object_id=content.id \
               AND finding.resolved_at IS NULL) \
     FROM content_objects AS content \
     WHERE content.collection_id=:collection_id \
       AND (:after_sha256 IS NULL OR content.sha256 > :after_sha256) \
       AND (:availability IS NULL OR content.availability_state=:availability) \
       AND (:size_state IS NULL OR content.size_state=:size_state) \
       AND (:finding_code IS NULL OR EXISTS ( \
            SELECT 1 FROM findings AS finding \
            WHERE finding.run_id=:run_id AND finding.content_object_id=content.id \
              AND finding.code=:finding_code AND finding.resolved_at IS NULL \
       )) \
       AND (:search_like IS NULL OR lower(content.sha256) LIKE :search_like \
            OR lower(content.source_detected_mime_type) LIKE :search_like \
            OR EXISTS (SELECT 1 FROM filename_variants AS variant \
                       WHERE variant.content_object_id=content.id \
                         AND lower(variant.display_filename) LIKE :search_like) \
            OR EXISTS (SELECT 1 FROM content_occurrences AS occurrence \
                       JOIN source_messages AS message \
                         ON message.snapshot_id=occurrence.snapshot_id \
                        AND message.source_message_id=occurrence.source_message_id \
                       WHERE occurrence.content_object_id=content.id \
                         AND (lower(COALESCE(occurrence.sender_domain, '')) LIKE :search_like \
                              OR lower(message.subject_raw) LIKE :search_like))) \
     ORDER BY content.sha256 LIMIT :fetch_limit"
}

fn inventory_object_by_id_sql() -> &'static str {
    "SELECT content.id, content.sha256, \
            COALESCE((SELECT variant.display_filename FROM filename_variants AS variant \
                      WHERE variant.content_object_id=content.id \
                      ORDER BY variant.occurrence_count DESC, variant.normalized_filename LIMIT 1), \
                     '[unnamed]'), \
            content.source_detected_mime_type, content.expected_size_bytes, \
            content.actual_size_bytes, content.occurrence_count, \
            (SELECT COUNT(*) FROM filename_variants AS variant \
             WHERE variant.content_object_id=content.id), \
            (SELECT COUNT(DISTINCT occurrence.source_message_id) \
             FROM content_occurrences AS occurrence \
             WHERE occurrence.content_object_id=content.id), \
            (SELECT COUNT(DISTINCT COALESCE(message.provider_thread_namespace, '') || ':' || \
                                   COALESCE(message.provider_thread_value, '')) \
             FROM content_occurrences AS occurrence \
             JOIN source_messages AS message \
               ON message.snapshot_id=occurrence.snapshot_id \
              AND message.source_message_id=occurrence.source_message_id \
             WHERE occurrence.content_object_id=content.id \
               AND message.provider_thread_value IS NOT NULL), \
            content.first_seen_at, content.last_seen_at, content.availability_state, \
            content.size_state, \
            (SELECT COUNT(*) FROM findings AS finding \
             WHERE finding.run_id=:run_id AND finding.content_object_id=content.id \
               AND finding.resolved_at IS NULL) \
     FROM content_objects AS content
     WHERE content.collection_id=:collection_id AND content.id=:content_object_id"
}

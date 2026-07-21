CREATE INDEX idx_content_objects_collection_mime_sha
    ON content_objects(collection_id, source_detected_mime_type, sha256);

CREATE INDEX idx_content_occurrences_sender_content
    ON content_occurrences(sender_domain, content_object_id)
    WHERE sender_domain IS NOT NULL;

CREATE INDEX idx_source_messages_snapshot_header_date
    ON source_messages(snapshot_id, header_date);

CREATE INDEX idx_source_messages_snapshot_thread
    ON source_messages(snapshot_id, provider_thread_namespace, provider_thread_value)
    WHERE provider_thread_value IS NOT NULL;

CREATE INDEX idx_findings_run_code_severity_id
    ON findings(run_id, code, severity, id)
    WHERE resolved_at IS NULL;

CREATE INDEX idx_findings_run_content
    ON findings(run_id, content_object_id)
    WHERE resolved_at IS NULL AND content_object_id IS NOT NULL;

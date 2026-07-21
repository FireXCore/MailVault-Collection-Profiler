mod file_stat;
pub mod workspace;

use std::{fs, path::PathBuf};

use profiler_adapter_mailvault::{MailVaultAdapter, MailVaultPhysicalObjectResolver};
use profiler_core::{
    CollectionAdapter, FileStatOptions, FileStatRequest, FileStatResult, InventoryOptions,
    InventoryRequest, InventoryResult, InventorySource, PreflightReport, ProfilerError,
    ProfilerResult, ProgressEvent, ProgressSink, ProgressUnit, RunStage, RunState, SnapshotOptions,
    SnapshotRequest, SnapshotResult, StageState,
};
use profiler_storage_sqlite::ProfilerStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOptions {
    pub snapshot: SnapshotOptions,
    pub inventory: InventoryOptions,
    pub file_stat: FileStatOptions,
}

#[derive(Debug, Clone)]
pub struct ProfileRequest {
    pub archive_root: PathBuf,
    pub workspace_root: PathBuf,
    pub options: ProfileOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResult {
    pub run_id: String,
    pub collection_id: String,
    pub source_snapshot_id: String,
    pub profiler_database: String,
    pub preflight: PreflightReport,
    pub snapshot: SnapshotResult,
    pub inventory: InventoryResult,
    pub file_stat: FileStatResult,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProfileEngine;

impl ProfileEngine {
    pub const PIPELINE_VERSION: &'static str = env!("CARGO_PKG_VERSION");

    pub fn profile(
        &self,
        request: &ProfileRequest,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<ProfileResult> {
        validate_request(request)?;
        fs::create_dir_all(&request.workspace_root).map_err(|source| ProfilerError::Io {
            operation: "creating profiler workspace",
            path: request.workspace_root.clone(),
            source,
        })?;

        let adapter = MailVaultAdapter;
        let preflight = adapter.preflight(&request.archive_root)?;
        if !preflight.compatible {
            return Err(ProfilerError::IncompatibleSource(format!(
                "preflight failed with {} required errors",
                preflight.errors_count
            )));
        }
        let archive_identity = preflight.archive_identity.as_deref().ok_or_else(|| {
            ProfilerError::IncompatibleSource(
                "preflight did not produce an archive identity".into(),
            )
        })?;

        let profiler_database = request
            .workspace_root
            .join("profiler")
            .join("profiler.sqlite3");
        let mut store = ProfilerStore::open(&profiler_database)?;
        let collection_id =
            store.register_collection(adapter.kind(), archive_identity, &preflight.archive_root)?;
        let run_id = Uuid::now_v7().to_string();
        let configuration_fingerprint = configuration_fingerprint(&request.options)?;
        store.create_run_with_id(
            &run_id,
            Some(&collection_id),
            Self::PIPELINE_VERSION,
            &configuration_fingerprint,
        )?;

        let result = Self::profile_registered_run(
            request,
            progress,
            adapter,
            &preflight,
            &mut store,
            &run_id,
            &collection_id,
            &profiler_database,
            &configuration_fingerprint,
        );
        if let Err(error) = &result {
            let _ = store.fail_run(&run_id, error);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn profile_registered_run(
        request: &ProfileRequest,
        progress: &dyn ProgressSink,
        adapter: MailVaultAdapter,
        preflight: &PreflightReport,
        store: &mut ProfilerStore,
        run_id: &str,
        collection_id: &str,
        profiler_database: &std::path::Path,
        configuration_fingerprint: &str,
    ) -> ProfilerResult<ProfileResult> {
        store.transition_run(run_id, RunState::Preflighting)?;
        progress.send(ProgressEvent {
            run_id: run_id.to_owned(),
            sequence: 1,
            stage: RunStage::Preflight,
            stage_state: StageState::Completed,
            unit: ProgressUnit::Checks,
            completed_items: u64::try_from(preflight.checks.len()).unwrap_or(u64::MAX),
            total_items: Some(u64::try_from(preflight.checks.len()).unwrap_or(u64::MAX)),
            completed_bytes: preflight.database_bytes,
            total_bytes: Some(preflight.database_bytes),
            elapsed_ms: 0,
            instant_throughput: None,
            smoothed_throughput: None,
            eta_ms: Some(0),
            active_workers: 0,
            queue_depth: 0,
            warnings: preflight.warnings_count,
            errors: preflight.errors_count,
            current_object_display: None,
            checkpoint_sequence: 1,
        })?;

        store.transition_run(run_id, RunState::Snapshotting)?;
        let snapshot = adapter.create_snapshot(
            &SnapshotRequest {
                run_id: run_id.to_owned(),
                archive_root: request.archive_root.clone(),
                workspace_root: request.workspace_root.clone(),
                options: request.options.snapshot,
            },
            progress,
        )?;
        let source_snapshot_id =
            store.register_source_snapshot(collection_id, &snapshot.manifest)?;
        store.attach_source_snapshot(run_id, &source_snapshot_id)?;
        store.transition_run(run_id, RunState::Ready)?;
        store.transition_run(run_id, RunState::Running)?;

        let inventory = adapter.inventory(
            &InventoryRequest {
                run_id: run_id.to_owned(),
                collection_id: collection_id.to_owned(),
                source_snapshot_id: source_snapshot_id.clone(),
                snapshot_database: PathBuf::from(&snapshot.manifest.snapshot_database),
                archive_root: request.archive_root.clone(),
                expected_metrics: snapshot.manifest.snapshot_metrics.clone(),
                options: request.options.inventory,
            },
            store,
            progress,
        )?;
        reconcile_inventory(preflight, &inventory)?;

        let resolver =
            MailVaultPhysicalObjectResolver::new(&request.archive_root).map_err(|source| {
                ProfilerError::Io {
                    operation: "canonicalizing MailVault archive root for file-stat",
                    path: request.archive_root.clone(),
                    source,
                }
            })?;
        let file_stat = file_stat::run_file_stat(
            &FileStatRequest {
                run_id: run_id.to_owned(),
                collection_id: collection_id.to_owned(),
                archive_root: request.archive_root.clone(),
                agent_name: "mailvault-path-inspector".into(),
                agent_version: MailVaultAdapter::ADAPTER_VERSION.into(),
                configuration_fingerprint: configuration_fingerprint.to_owned(),
                options: request.options.file_stat,
            },
            store,
            &resolver,
            progress,
        )?;
        store.transition_run(run_id, RunState::Succeeded)?;

        Ok(ProfileResult {
            run_id: run_id.to_owned(),
            collection_id: collection_id.to_owned(),
            source_snapshot_id,
            profiler_database: profiler_database.to_string_lossy().into_owned(),
            preflight: preflight.clone(),
            snapshot,
            inventory,
            file_stat,
        })
    }
}

fn validate_request(request: &ProfileRequest) -> ProfilerResult<()> {
    if !request.archive_root.is_dir() {
        return Err(ProfilerError::InvalidPath {
            message: "MailVault archive root is not a directory".into(),
            path: request.archive_root.clone(),
        });
    }
    if request.options.inventory.batch_size == 0 {
        return Err(ProfilerError::InvalidArgument(
            "inventory batch size must be greater than zero".into(),
        ));
    }
    if request.options.file_stat.batch_size == 0 {
        return Err(ProfilerError::InvalidArgument(
            "file-stat batch size must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn reconcile_inventory(
    preflight: &PreflightReport,
    result: &InventoryResult,
) -> ProfilerResult<()> {
    let expected = &preflight.metrics;
    let actual = &result.summary;
    let mismatches = [
        ("messages", actual.messages, expected.messages),
        (
            "message_occurrences",
            actual.message_occurrences,
            expected.message_occurrences,
        ),
        ("participants", actual.participants, expected.participants),
        ("parts", actual.parts, expected.mime_parts),
        (
            "attachment_occurrences",
            actual.attachment_occurrences,
            expected.attachment_occurrences,
        ),
        ("blob_rows", actual.blob_rows, expected.blobs),
        (
            "message_relations",
            actual.message_relations,
            expected.message_relations,
        ),
    ]
    .into_iter()
    .filter(|(_, actual, expected)| actual != expected)
    .map(|(name, actual, expected)| format!("{name}: actual={actual}, expected={expected}"))
    .collect::<Vec<_>>();

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(ProfilerError::IncompatibleSource(format!(
            "physical inventory reconciliation failed: {}",
            mismatches.join("; ")
        )))
    }
}

fn configuration_fingerprint(options: &ProfileOptions) -> ProfilerResult<String> {
    let payload = serde_json::to_vec(options).map_err(|error| {
        ProfilerError::Internal(format!("serializing profile configuration: {error}"))
    })?;
    let mut digest = Sha256::new();
    digest.update(b"mailvault-profiler-configuration-v1\0");
    digest.update(payload);
    Ok(hex::encode(digest.finalize()))
}

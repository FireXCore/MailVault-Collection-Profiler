use std::{path::PathBuf, str::FromStr, sync::Mutex};

use profiler_adapter_mailvault::MailVaultAdapter;
use profiler_core::{
    ActiveRunContext, CollectionAdapter, ContentObjectDetail, ErrorReport, FindingCategory,
    FindingDetail, FindingReviewHistory, FindingsPage, FindingsPageRequest, InventoryFilters,
    InventoryPage, InventoryPageRequest, OpenWorkspaceResult, PreflightReport, ProfilerError,
    ProfilerResult, ProgressEvent, ProgressSink, ReviewActorKind, ReviewStatus, RunCatalogEntry,
    SnapshotOptions, SnapshotRequest, SnapshotResult, WorkspaceInspection, WorkspaceOpenMode,
};
use profiler_engine::{
    ProfileEngine, ProfileOptions, ProfileRequest, ProfileResult,
    workspace::{
        WorkspaceContext, WorkspaceSession, add_review_note, clear_review_status,
        export_sanitized_run, finding_detail, findings_page as query_findings_page,
        inventory_page as query_inventory_page, list_runs, open_run, set_review_status,
    },
};
use serde::Deserialize;
use tauri::{State, ipc::Channel};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindingsPageCommandRequest {
    code: Option<String>,
    severity: Option<String>,
    review_status: Option<String>,
    category: Option<FindingCategory>,
    search: Option<String>,
    after_id: Option<String>,
    limit: u32,
}

struct ChannelProgressSink {
    channel: Channel<ProgressEvent>,
}

impl ProgressSink for ChannelProgressSink {
    fn send(&self, event: ProgressEvent) -> ProfilerResult<()> {
        self.channel
            .send(event)
            .map_err(|error| ProfilerError::ProgressDelivery(error.to_string()))
    }
}

#[derive(Debug)]
struct DesktopWorkspace {
    session: WorkspaceSession,
    active_run: Option<ActiveRunContext>,
}

#[derive(Debug, Default)]
struct DesktopState {
    workspace: Mutex<Option<DesktopWorkspace>>,
}

impl DesktopState {
    fn replace_workspace(
        &self,
        session: WorkspaceSession,
        active_run: Option<ActiveRunContext>,
    ) -> ProfilerResult<OpenWorkspaceResult> {
        let result = session.open_result()?;
        let mut state = self
            .workspace
            .lock()
            .map_err(|_| ProfilerError::Internal("desktop workspace state is poisoned".into()))?;
        *state = Some(DesktopWorkspace {
            session,
            active_run,
        });
        Ok(result)
    }

    fn close_workspace(&self) -> ProfilerResult<()> {
        let mut state = self
            .workspace
            .lock()
            .map_err(|_| ProfilerError::Internal("desktop workspace state is poisoned".into()))?;
        *state = None;
        Ok(())
    }

    fn context(&self) -> ProfilerResult<WorkspaceContext> {
        self.workspace
            .lock()
            .map_err(|_| ProfilerError::Internal("desktop workspace state is poisoned".into()))?
            .as_ref()
            .map(|workspace| workspace.session.context())
            .ok_or_else(|| {
                ProfilerError::contract(
                    profiler_core::ErrorCode::WorkspaceNotFound,
                    "no workspace is open in this desktop session",
                    false,
                )
            })
    }

    fn active_run(&self) -> ProfilerResult<ActiveRunContext> {
        self.workspace
            .lock()
            .map_err(|_| ProfilerError::Internal("desktop workspace state is poisoned".into()))?
            .as_ref()
            .and_then(|workspace| workspace.active_run.clone())
            .ok_or_else(|| {
                ProfilerError::contract(
                    profiler_core::ErrorCode::RunNotFound,
                    "no profiler run is active in this desktop session",
                    false,
                )
            })
    }

    fn open_result(&self) -> ProfilerResult<OpenWorkspaceResult> {
        self.workspace
            .lock()
            .map_err(|_| ProfilerError::Internal("desktop workspace state is poisoned".into()))?
            .as_ref()
            .map(|workspace| workspace.session.open_result())
            .transpose()?
            .ok_or_else(|| {
                ProfilerError::contract(
                    profiler_core::ErrorCode::WorkspaceNotFound,
                    "no workspace is open in this desktop session",
                    false,
                )
            })
    }

    fn activate_run(&self, run: ActiveRunContext) -> ProfilerResult<()> {
        let mut state = self
            .workspace
            .lock()
            .map_err(|_| ProfilerError::Internal("desktop workspace state is poisoned".into()))?;
        let workspace = state.as_mut().ok_or_else(|| {
            ProfilerError::contract(
                profiler_core::ErrorCode::WorkspaceNotFound,
                "no workspace is open in this desktop session",
                false,
            )
        })?;
        workspace.active_run = Some(run);
        Ok(())
    }
}

#[tauri::command]
fn preflight_archive(root: String) -> Result<PreflightReport, ErrorReport> {
    MailVaultAdapter
        .preflight(&PathBuf::from(root))
        .map_err(|error| error.report())
}

#[tauri::command]
fn inspect_workspace(path: String) -> Result<WorkspaceInspection, ErrorReport> {
    WorkspaceSession::inspect(&PathBuf::from(path)).map_err(|error| error.report())
}

#[tauri::command]
async fn open_workspace(
    path: String,
    read_only: bool,
    allow_migration: bool,
    state: State<'_, DesktopState>,
) -> Result<OpenWorkspaceResult, ErrorReport> {
    let session = tauri::async_runtime::spawn_blocking(move || {
        WorkspaceSession::open(
            &PathBuf::from(path),
            if read_only {
                WorkspaceOpenMode::ReadOnly
            } else {
                WorkspaceOpenMode::ReadWritePreferred
            },
            allow_migration,
        )
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("workspace worker failed: {error}")).report())?
    .map_err(|error| error.report())?;
    state
        .replace_workspace(session, None)
        .map_err(|error| error.report())
}

// Tauri injects managed state through the owned `State<T>` command argument.
// `&State<T>` does not implement Tauri's command extraction contract, so this
// framework-boundary exception is intentionally limited to these sync commands.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
fn current_workspace(state: State<'_, DesktopState>) -> Result<OpenWorkspaceResult, ErrorReport> {
    state.open_result().map_err(|error| error.report())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
fn close_workspace(state: State<'_, DesktopState>) -> Result<(), ErrorReport> {
    state.close_workspace().map_err(|error| error.report())
}

#[tauri::command]
async fn workspace_runs(
    state: State<'_, DesktopState>,
) -> Result<Vec<RunCatalogEntry>, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || list_runs(&context))
        .await
        .map_err(|error| {
            ProfilerError::Internal(format!("run catalog worker failed: {error}")).report()
        })?
        .map_err(|error| error.report())
}

#[tauri::command]
async fn open_existing_run(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<ActiveRunContext, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let run = tauri::async_runtime::spawn_blocking(move || open_run(&context, &run_id))
        .await
        .map_err(|error| ProfilerError::Internal(format!("run opener failed: {error}")).report())?
        .map_err(|error| error.report())?;
    state
        .activate_run(run.clone())
        .map_err(|error| error.report())?;
    Ok(run)
}

#[tauri::command]
async fn create_source_snapshot(
    root: String,
    workspace: String,
    on_event: Channel<ProgressEvent>,
) -> Result<SnapshotResult, ErrorReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let request = SnapshotRequest {
            run_id: Uuid::now_v7().to_string(),
            archive_root: PathBuf::from(root),
            workspace_root: PathBuf::from(workspace),
            options: SnapshotOptions::default(),
        };
        MailVaultAdapter.create_snapshot(&request, &ChannelProgressSink { channel: on_event })
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("snapshot worker failed: {error}")).report())?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn profile_collection(
    root: String,
    workspace: String,
    on_event: Channel<ProgressEvent>,
    state: State<'_, DesktopState>,
) -> Result<ProfileResult, ErrorReport> {
    let workspace_path = PathBuf::from(&workspace);
    let result = tauri::async_runtime::spawn_blocking(move || {
        ProfileEngine.profile(
            &ProfileRequest {
                archive_root: PathBuf::from(root),
                workspace_root: PathBuf::from(workspace),
                options: ProfileOptions::default(),
            },
            &ChannelProgressSink { channel: on_event },
        )
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("profile worker failed: {error}")).report())?
    .map_err(|error| error.report())?;

    let session =
        WorkspaceSession::open(&workspace_path, WorkspaceOpenMode::ReadWritePreferred, true)
            .map_err(|error| error.report())?;
    let context = session.context();
    let active_run = open_run(&context, &result.run_id).map_err(|error| error.report())?;
    state
        .replace_workspace(session, Some(active_run))
        .map_err(|error| error.report())?;
    Ok(result)
}

#[tauri::command]
async fn inventory_page(
    filters: InventoryFilters,
    after_sha256: Option<String>,
    limit: u32,
    state: State<'_, DesktopState>,
) -> Result<InventoryPage, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        query_inventory_page(
            &context,
            &InventoryPageRequest {
                collection_id: active.collection_id,
                run_id: active.run.run_id,
                filters,
                after_sha256,
                limit,
            },
        )
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("inventory reader failed: {error}")).report())?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn content_object_detail(
    content_object_id: String,
    state: State<'_, DesktopState>,
) -> Result<ContentObjectDetail, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        context.open_reader()?.content_object_detail(
            &active.run.run_id,
            &active.collection_id,
            &content_object_id,
        )
    })
    .await
    .map_err(|error| {
        ProfilerError::Internal(format!("content detail reader failed: {error}")).report()
    })?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn findings_page(
    request: FindingsPageCommandRequest,
    state: State<'_, DesktopState>,
) -> Result<FindingsPage, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        query_findings_page(
            &context,
            &FindingsPageRequest {
                run_id: active.run.run_id,
                code: request.code,
                severity: request.severity,
                review_status: request.review_status,
                category: request.category,
                search: request.search,
                after_id: request.after_id,
                limit: request.limit,
            },
        )
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("findings reader failed: {error}")).report())?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn get_finding_detail(
    finding_id: String,
    state: State<'_, DesktopState>,
) -> Result<FindingDetail, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        finding_detail(&context, &active.run.run_id, &finding_id)
    })
    .await
    .map_err(|error| {
        ProfilerError::Internal(format!("finding detail worker failed: {error}")).report()
    })?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn set_finding_review_status(
    finding_id: String,
    status: String,
    note: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<FindingReviewHistory, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    let status = ReviewStatus::from_str(&status).map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        set_review_status(
            &context,
            &active.run.run_id,
            &finding_id,
            status,
            note.as_deref(),
            ReviewActorKind::LocalInteractiveUser,
        )
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("review writer failed: {error}")).report())?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn clear_finding_review_status(
    finding_id: String,
    note: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<FindingReviewHistory, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        clear_review_status(
            &context,
            &active.run.run_id,
            &finding_id,
            note.as_deref(),
            ReviewActorKind::LocalInteractiveUser,
        )
    })
    .await
    .map_err(|error| ProfilerError::Internal(format!("review clearer failed: {error}")).report())?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn add_finding_review_note(
    finding_id: String,
    note: String,
    state: State<'_, DesktopState>,
) -> Result<FindingReviewHistory, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    let active = state.active_run().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        add_review_note(
            &context,
            &active.run.run_id,
            &finding_id,
            &note,
            ReviewActorKind::LocalInteractiveUser,
        )
    })
    .await
    .map_err(|error| {
        ProfilerError::Internal(format!("review note writer failed: {error}")).report()
    })?
    .map_err(|error| error.report())
}

#[tauri::command]
async fn export_sanitized_summary(
    run_id: String,
    destination: String,
    state: State<'_, DesktopState>,
) -> Result<String, ErrorReport> {
    let context = state.context().map_err(|error| error.report())?;
    tauri::async_runtime::spawn_blocking(move || {
        export_sanitized_run(&context, &run_id, &PathBuf::from(destination))
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| {
        ProfilerError::Internal(format!("sanitized export worker failed: {error}")).report()
    })?
    .map_err(|error| error.report())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mailvault_profiler=info".into()),
        )
        .json()
        .init();

    tauri::Builder::default()
        .manage(DesktopState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            preflight_archive,
            inspect_workspace,
            open_workspace,
            current_workspace,
            close_workspace,
            workspace_runs,
            open_existing_run,
            create_source_snapshot,
            profile_collection,
            inventory_page,
            content_object_detail,
            findings_page,
            get_finding_detail,
            set_finding_review_status,
            clear_finding_review_status,
            add_finding_review_note,
            export_sanitized_summary
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            eprintln!("failed to run MailVault Collection Profiler: {error}");
            std::process::exit(1);
        });
}

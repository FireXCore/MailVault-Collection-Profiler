use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
};

use clap::{Parser, Subcommand};
use profiler_adapter_mailvault::MailVaultAdapter;
use profiler_core::{
    CollectionAdapter, FindingCategory, FindingsPageRequest, FormatFilters,
    FormatIdentificationRequest, FormatOptions, FormatPageRequest, FormatState, ProfilerResult,
    ProgressEvent, ProgressSink, ReviewActorKind, ReviewStatus, SnapshotOptions, SnapshotRequest,
    WorkspaceOpenMode,
};
use profiler_engine::{
    ExactFormatEngine, ProfileEngine, ProfileOptions, ProfileRequest,
    workspace::{
        WorkspaceSession, add_review_note, clear_review_status, export_sanitized_run,
        finding_detail, findings_page, list_runs, set_review_status,
    },
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "mailvault-profiler", version, about)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Preflight {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Snapshot {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run_id: Option<String>,
    },
    Profile {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long, default_value_t = 1_000)]
        batch_size: u32,
        /// Zero selects the conservative provisional auto policy.
        #[arg(long, default_value_t = 0)]
        file_stat_workers: u32,
        #[arg(long, default_value_t = 512)]
        file_stat_batch_size: u32,
    },
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    Runs {
        #[command(subcommand)]
        command: RunsCommand,
    },
    Findings {
        #[command(subcommand)]
        command: FindingsCommand,
    },
    Formats {
        #[command(subcommand)]
        command: FormatsCommand,
    },
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    Inspect {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RunsCommand {
    List {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum FindingsCommand {
    List {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        code: Option<String>,
        #[arg(long)]
        review_status: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "all")]
        category: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
        #[arg(long)]
        json: bool,
    },
    Show {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        finding: String,
        #[arg(long)]
        json: bool,
    },
    Review {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        finding: String,
        #[arg(long)]
        status: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        allow_migration: bool,
    },
    Clear {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        finding: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        allow_migration: bool,
    },
    Note {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        finding: String,
        #[arg(long)]
        note: String,
        #[arg(long)]
        allow_migration: bool,
    },
}

#[derive(Debug, Subcommand)]
enum FormatsCommand {
    Probe {
        #[arg(long)]
        siegfried: Option<PathBuf>,
        #[arg(long)]
        signature: Option<PathBuf>,
        #[arg(long, default_value_t = 0)]
        workers: u32,
        #[arg(long)]
        json: bool,
    },
    Identify {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        siegfried: Option<PathBuf>,
        #[arg(long)]
        signature: Option<PathBuf>,
        #[arg(long, default_value_t = 2_048)]
        batch_size: u32,
        #[arg(long, default_value_t = 0)]
        workers: u32,
        #[arg(long, default_value_t = 900)]
        timeout_seconds: u64,
        #[arg(long, default_value_t = true)]
        resume: bool,
        #[arg(long)]
        allow_migration: bool,
    },
    Summary {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        puid: Option<String>,
        #[arg(long)]
        mismatch_only: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: u32,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug)]
struct IdentifyFormatsArgs {
    workspace: PathBuf,
    run: String,
    siegfried: Option<PathBuf>,
    signature: Option<PathBuf>,
    batch_size: u32,
    workers: u32,
    timeout_seconds: u64,
    resume: bool,
    allow_migration: bool,
}

#[derive(Debug)]
struct ListFormatsArgs {
    workspace: PathBuf,
    run: String,
    state: Option<String>,
    puid: Option<String>,
    mismatch_only: bool,
    search: Option<String>,
    limit: u32,
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    SanitizedSummary {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug)]
struct JsonLineProgress;

impl ProgressSink for JsonLineProgress {
    fn send(&self, event: ProgressEvent) -> ProfilerResult<()> {
        eprintln!(
            "{}",
            serde_json::to_string(&event).map_err(|error| {
                profiler_core::ProfilerError::ProgressDelivery(error.to_string())
            })?
        );
        Ok(())
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mailvault_profiler=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    match run(Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&error.report()).unwrap_or_else(|_| error.to_string())
            );
            ExitCode::from(2)
        }
    }
}

fn run(arguments: Arguments) -> ProfilerResult<()> {
    match arguments.command {
        Command::Preflight { archive, json } => run_preflight(&archive, json),
        Command::Snapshot {
            archive,
            workspace,
            run_id,
        } => run_snapshot(archive, workspace, run_id),
        Command::Profile {
            archive,
            workspace,
            batch_size,
            file_stat_workers,
            file_stat_batch_size,
        } => run_profile(
            archive,
            workspace,
            batch_size,
            file_stat_workers,
            file_stat_batch_size,
        ),
        Command::Workspace { command } => run_workspace_command(command),
        Command::Runs { command } => run_runs_command(command),
        Command::Findings { command } => run_findings_command(command),
        Command::Formats { command } => run_formats_command(command),
        Command::Export { command } => run_export_command(command),
    }
}

fn run_preflight(archive: &Path, json: bool) -> ProfilerResult<()> {
    let report = MailVaultAdapter.preflight(archive)?;
    if json {
        print_json(&report)?;
    } else {
        println!("Archive: {}", report.archive_root);
        println!("Compatible: {}", report.compatible);
        println!("Schema: {:?}", report.schema_version);
        println!("Messages: {}", report.metrics.messages);
        println!("MIME parts: {}", report.metrics.mime_parts);
        println!("Attachments: {}", report.metrics.attachment_occurrences);
        println!("Blobs: {}", report.metrics.blobs);
        for check in &report.checks {
            println!(
                "[{status:?}] {label}: {detail}",
                status = check.status,
                label = check.label,
                detail = check.detail
            );
        }
    }
    if report.compatible {
        Ok(())
    } else {
        Err(profiler_core::ProfilerError::IncompatibleSource(
            "preflight did not pass".into(),
        ))
    }
}

fn run_snapshot(
    archive: PathBuf,
    workspace: PathBuf,
    run_id: Option<String>,
) -> ProfilerResult<()> {
    let request = SnapshotRequest {
        run_id: run_id.unwrap_or_else(|| Uuid::now_v7().to_string()),
        archive_root: archive,
        workspace_root: workspace,
        options: SnapshotOptions::default(),
    };
    print_json(&MailVaultAdapter.create_snapshot(&request, &JsonLineProgress)?)
}

fn run_profile(
    archive: PathBuf,
    workspace: PathBuf,
    batch_size: u32,
    file_stat_workers: u32,
    file_stat_batch_size: u32,
) -> ProfilerResult<()> {
    let mut options = ProfileOptions::default();
    options.inventory.batch_size = batch_size;
    options.file_stat.workers = file_stat_workers;
    options.file_stat.batch_size = file_stat_batch_size;
    print_json(&ProfileEngine.profile(
        &ProfileRequest {
            archive_root: archive,
            workspace_root: workspace,
            options,
        },
        &JsonLineProgress,
    )?)
}

fn run_workspace_command(command: WorkspaceCommand) -> ProfilerResult<()> {
    match command {
        WorkspaceCommand::Inspect { workspace, json } => {
            let inspection = WorkspaceSession::inspect(&workspace)?;
            if json {
                print_json(&inspection)
            } else {
                println!("Workspace: {}", inspection.root_path.display());
                println!("Compatibility: {:?}", inspection.compatibility);
                println!("Schema: {:?}", inspection.schema_version);
                println!("Supported schema: {}", inspection.supported_schema_version);
                println!("Migration required: {}", inspection.migration_required);
                println!("Writer lock active: {}", inspection.lock_active);
                println!("Runs: {}", inspection.run_count);
                println!("Detail: {}", inspection.detail);
                Ok(())
            }
        }
    }
}

fn run_runs_command(command: RunsCommand) -> ProfilerResult<()> {
    match command {
        RunsCommand::List { workspace, json } => {
            let session = open_read_only_workspace(&workspace)?;
            let runs = list_runs(&session.context())?;
            if json {
                print_json(&runs)
            } else {
                for run in runs {
                    println!(
                        "{}  {:?}  {}  findings={} review={}%",
                        run.run_id,
                        run.status,
                        run.started_at,
                        run.findings,
                        run.review_summary.review_completion_percent
                    );
                }
                Ok(())
            }
        }
    }
}

fn run_findings_command(command: FindingsCommand) -> ProfilerResult<()> {
    match command {
        FindingsCommand::List {
            workspace,
            run,
            severity,
            code,
            review_status,
            search,
            category,
            limit,
            json,
        } => {
            let request = FindingsPageRequest {
                run_id: run,
                code,
                severity,
                review_status,
                category: Some(parse_category(&category)?),
                search,
                after_id: None,
                limit,
            };
            run_findings_list(&workspace, &request, json)
        }
        FindingsCommand::Show {
            workspace,
            run,
            finding,
            json,
        } => run_finding_show(&workspace, &run, &finding, json),
        FindingsCommand::Review {
            workspace,
            run,
            finding,
            status,
            note,
            allow_migration,
        } => run_finding_review(
            &workspace,
            &run,
            &finding,
            &status,
            note.as_deref(),
            allow_migration,
        ),
        FindingsCommand::Clear {
            workspace,
            run,
            finding,
            note,
            allow_migration,
        } => run_finding_clear(&workspace, &run, &finding, note.as_deref(), allow_migration),
        FindingsCommand::Note {
            workspace,
            run,
            finding,
            note,
            allow_migration,
        } => run_finding_note(&workspace, &run, &finding, &note, allow_migration),
    }
}

fn run_findings_list(
    workspace: &Path,
    request: &FindingsPageRequest,
    json: bool,
) -> ProfilerResult<()> {
    let session = open_read_only_workspace(workspace)?;
    let page = findings_page(&session.context(), request)?;
    if json {
        print_json(&page)
    } else {
        for finding in page.items {
            let review_status = finding
                .review_status
                .map_or_else(|| "unreviewed".to_owned(), |status| status.to_string());
            println!(
                "{}  {}  {}  review={review_status}",
                finding.severity, finding.code, finding.id
            );
        }
        Ok(())
    }
}

fn run_finding_show(
    workspace: &Path,
    run_id: &str,
    finding_id: &str,
    json: bool,
) -> ProfilerResult<()> {
    let session = open_read_only_workspace(workspace)?;
    let detail = finding_detail(&session.context(), run_id, finding_id)?;
    if json {
        print_json(&detail)
    } else {
        println!("Finding: {}", detail.finding.id);
        println!("Code: {}", detail.finding.code);
        println!("Severity: {}", detail.finding.severity);
        println!("Message: {}", detail.finding.message);
        println!(
            "Review: {}",
            detail
                .review
                .current_status
                .map_or_else(|| "unreviewed".to_owned(), |status| status.to_string())
        );
        println!("Review events: {}", detail.review.events.len());
        println!("Review integrity: {}", detail.review.integrity_valid);
        Ok(())
    }
}

fn run_finding_review(
    workspace: &Path,
    run_id: &str,
    finding_id: &str,
    status: &str,
    note: Option<&str>,
    allow_migration: bool,
) -> ProfilerResult<()> {
    let session = open_review_workspace(workspace, allow_migration)?;
    print_json(&set_review_status(
        &session.context(),
        run_id,
        finding_id,
        ReviewStatus::from_str(status)?,
        note,
        ReviewActorKind::LocalCliUser,
    )?)
}

fn run_finding_clear(
    workspace: &Path,
    run_id: &str,
    finding_id: &str,
    note: Option<&str>,
    allow_migration: bool,
) -> ProfilerResult<()> {
    let session = open_review_workspace(workspace, allow_migration)?;
    print_json(&clear_review_status(
        &session.context(),
        run_id,
        finding_id,
        note,
        ReviewActorKind::LocalCliUser,
    )?)
}

fn run_finding_note(
    workspace: &Path,
    run_id: &str,
    finding_id: &str,
    note: &str,
    allow_migration: bool,
) -> ProfilerResult<()> {
    let session = open_review_workspace(workspace, allow_migration)?;
    print_json(&add_review_note(
        &session.context(),
        run_id,
        finding_id,
        note,
        ReviewActorKind::LocalCliUser,
    )?)
}

fn run_formats_command(command: FormatsCommand) -> ProfilerResult<()> {
    match command {
        FormatsCommand::Probe {
            siegfried,
            signature,
            workers,
            json,
        } => run_format_probe(siegfried, signature, workers, json),
        FormatsCommand::Identify {
            workspace,
            run,
            siegfried,
            signature,
            batch_size,
            workers,
            timeout_seconds,
            resume,
            allow_migration,
        } => run_format_identification(IdentifyFormatsArgs {
            workspace,
            run,
            siegfried,
            signature,
            batch_size,
            workers,
            timeout_seconds,
            resume,
            allow_migration,
        }),
        FormatsCommand::Summary {
            workspace,
            run,
            json,
        } => run_format_summary(&workspace, &run, json),
        FormatsCommand::List {
            workspace,
            run,
            state,
            puid,
            mismatch_only,
            search,
            limit,
            json,
        } => run_format_list(ListFormatsArgs {
            workspace,
            run,
            state,
            puid,
            mismatch_only,
            search,
            limit,
            json,
        }),
    }
}

fn run_format_probe(
    siegfried: Option<PathBuf>,
    signature: Option<PathBuf>,
    workers: u32,
    json: bool,
) -> ProfilerResult<()> {
    let identity = ExactFormatEngine.probe_tool(siegfried, signature, workers)?;
    if json {
        print_json(&identity)
    } else {
        println!("Tool: {} {}", identity.tool_name, identity.tool_version);
        println!("Executable SHA-256: {}", identity.executable_sha256);
        println!("Signature: {}", identity.signature_path);
        println!("PRONOM signature version: {}", identity.signature_version);
        Ok(())
    }
}

fn run_format_identification(args: IdentifyFormatsArgs) -> ProfilerResult<()> {
    let session = open_review_workspace(&args.workspace, args.allow_migration)?;
    let context = session.context();
    let archive_root = single_source_root(&context.source_roots)?;

    print_json(&ExactFormatEngine.identify(
        &FormatIdentificationRequest {
            baseline_run_id: args.run,
            workspace_root: context.root_path,
            profiler_database: context.profiler_database,
            archive_root,
            siegfried_path: args.siegfried,
            signature_path: args.signature,
            options: FormatOptions {
                batch_size: args.batch_size,
                workers: args.workers,
                timeout_seconds: args.timeout_seconds,
                resume: args.resume,
            },
        },
        &JsonLineProgress,
    )?)
}

fn run_format_summary(workspace: &Path, run: &str, json: bool) -> ProfilerResult<()> {
    let session = open_read_only_workspace(workspace)?;
    let summary = ExactFormatEngine.summary(&session.context().profiler_database, run)?;

    if json {
        print_json(&summary)
    } else {
        println!("Baseline run: {}", summary.baseline_run_id);
        println!(
            "Latest format run: {}",
            summary.latest_format_run_id.as_deref().unwrap_or("none")
        );
        println!(
            "State: {}",
            summary.latest_run_state.as_deref().unwrap_or("not_started")
        );
        println!("Identified: {}", summary.identified);
        println!("Unknown: {}", summary.unknown);
        println!("Ambiguous: {}", summary.ambiguous);
        println!("Tool errors: {}", summary.tool_errors);
        println!("Extension mismatches: {}", summary.extension_mismatches);
        Ok(())
    }
}

fn run_format_list(args: ListFormatsArgs) -> ProfilerResult<()> {
    let session = open_read_only_workspace(&args.workspace)?;
    let page = ExactFormatEngine.page(
        &session.context().profiler_database,
        &FormatPageRequest {
            baseline_run_id: args.run,
            filters: FormatFilters {
                search: args.search,
                state: args.state.as_deref().map(parse_format_state).transpose()?,
                puid: args.puid,
                mismatch_only: args.mismatch_only,
            },
            after_sha256: None,
            limit: args.limit,
        },
    )?;

    if args.json {
        print_json(&page)
    } else {
        for item in page.items {
            println!(
                "{}  {}  {}  {}",
                item.state,
                item.primary_identifier.as_deref().unwrap_or("—"),
                item.primary_format_name.as_deref().unwrap_or("—"),
                item.sha256
            );
        }
        Ok(())
    }
}

fn parse_format_state(value: &str) -> ProfilerResult<FormatState> {
    match value.trim().to_ascii_lowercase().as_str() {
        "uninspected" => Ok(FormatState::Uninspected),
        "identified" => Ok(FormatState::Identified),
        "unknown" => Ok(FormatState::Unknown),
        "ambiguous" => Ok(FormatState::Ambiguous),
        "empty" => Ok(FormatState::Empty),
        "skipped_unavailable" => Ok(FormatState::SkippedUnavailable),
        "tool_error" => Ok(FormatState::ToolError),
        _ => Err(profiler_core::ProfilerError::InvalidArgument(
            "format state is not supported".into(),
        )),
    }
}

fn single_source_root(roots: &[PathBuf]) -> ProfilerResult<PathBuf> {
    match roots {
        [root] => Ok(root.clone()),
        [] => Err(profiler_core::ProfilerError::IncompatibleSource(
            "workspace does not contain a MailVault source root".into(),
        )),
        _ => Err(profiler_core::ProfilerError::IncompatibleSource(
            "workspace contains multiple source roots; select a single collection workspace".into(),
        )),
    }
}

fn run_export_command(command: ExportCommand) -> ProfilerResult<()> {
    match command {
        ExportCommand::SanitizedSummary {
            workspace,
            run,
            output,
        } => {
            let session = open_read_only_workspace(&workspace)?;
            let path = export_sanitized_run(&session.context(), &run, &output)?;
            println!("{}", path.display());
            Ok(())
        }
    }
}

fn open_read_only_workspace(workspace: &Path) -> ProfilerResult<WorkspaceSession> {
    WorkspaceSession::open(workspace, WorkspaceOpenMode::ReadOnly, false)
}

fn open_review_workspace(
    workspace: &Path,
    allow_migration: bool,
) -> ProfilerResult<WorkspaceSession> {
    WorkspaceSession::open(
        workspace,
        WorkspaceOpenMode::ReadWritePreferred,
        allow_migration,
    )
}

fn parse_category(value: &str) -> ProfilerResult<FindingCategory> {
    match value.trim().to_ascii_lowercase().as_str() {
        "requires_attention" | "attention" => Ok(FindingCategory::RequiresAttention),
        "informational_evidence" | "informational" | "info" => {
            Ok(FindingCategory::InformationalEvidence)
        }
        "reviewed" => Ok(FindingCategory::Reviewed),
        "all" => Ok(FindingCategory::All),
        _ => Err(profiler_core::ProfilerError::InvalidArgument(
            "finding category must be attention, informational, reviewed or all".into(),
        )),
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> ProfilerResult<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| {
            profiler_core::ProfilerError::Internal(format!("serializing CLI output: {error}"))
        })?
    );
    Ok(())
}

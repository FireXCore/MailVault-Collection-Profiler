import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import type {
  ActiveRunContext,
  ContentObjectDetail,
  ErrorReport,
  FindingCategory,
  FindingDetail,
  FindingReviewHistory,
  FindingsPage,
  InventoryFilters,
  InventoryPage,
  OpenWorkspaceResult,
  PreflightReport,
  ProfileResult,
  ProgressEvent,
  ReviewStatus,
  RunCatalogEntry,
  WorkspaceInspection,
} from './types';

export async function chooseDirectory(title: string): Promise<string | null> {
  if (!isTauri()) {
    throw new Error('Native folder selection is available only inside the desktop application.');
  }
  const selection = await open({ directory: true, multiple: false, title });
  return typeof selection === 'string' ? selection : null;
}

export async function chooseExportPath(
  title: string,
  defaultPath: string,
  extension: 'json' | 'csv',
): Promise<string | null> {
  if (!isTauri()) {
    throw new Error('Native export selection is available only inside the desktop application.');
  }
  return save({
    title,
    defaultPath,
    filters: [
      {
        name: extension === 'json' ? 'JSON summary' : 'CSV findings',
        extensions: [extension],
      },
    ],
  });
}

export async function preflightArchive(root: string): Promise<PreflightReport> {
  return invoke<PreflightReport>('preflight_archive', { root });
}

export async function inspectWorkspace(path: string): Promise<WorkspaceInspection> {
  return invoke<WorkspaceInspection>('inspect_workspace', { path });
}

export async function openWorkspace(
  path: string,
  readOnly: boolean,
  allowMigration: boolean,
): Promise<OpenWorkspaceResult> {
  return invoke<OpenWorkspaceResult>('open_workspace', { path, readOnly, allowMigration });
}

export async function getCurrentWorkspace(): Promise<OpenWorkspaceResult> {
  return invoke<OpenWorkspaceResult>('current_workspace');
}

export async function closeWorkspace(): Promise<void> {
  return invoke('close_workspace');
}

export async function getWorkspaceRuns(): Promise<RunCatalogEntry[]> {
  return invoke<RunCatalogEntry[]>('workspace_runs');
}

export async function openExistingRun(runId: string): Promise<ActiveRunContext> {
  return invoke<ActiveRunContext>('open_existing_run', { runId });
}

export async function createSourceSnapshot(
  root: string,
  workspace: string,
  onProgress: (event: ProgressEvent) => void,
): Promise<unknown> {
  const onEvent = new Channel<ProgressEvent>();
  onEvent.onmessage = onProgress;
  return invoke('create_source_snapshot', { root, workspace, onEvent });
}

export function normalizeError(error: unknown): ErrorReport {
  if (typeof error === 'object' && error !== null && 'message' in error) {
    const candidate = error as Partial<ErrorReport>;
    return {
      code: candidate.code ?? 'unknown',
      message: String(candidate.message),
      retryable: candidate.retryable ?? false,
      ...(candidate.context ? { context: candidate.context } : {}),
    };
  }
  return {
    code: 'unknown',
    message: String(error),
    retryable: false,
  };
}

export async function profileCollection(
  root: string,
  workspace: string,
  onProgress: (event: ProgressEvent) => void,
): Promise<ProfileResult> {
  const onEvent = new Channel<ProgressEvent>();
  onEvent.onmessage = onProgress;
  return invoke<ProfileResult>('profile_collection', { root, workspace, onEvent });
}

export async function getInventoryPage(
  filters: InventoryFilters,
  afterSha256: string | null,
  limit = 100,
): Promise<InventoryPage> {
  return invoke<InventoryPage>('inventory_page', { filters, afterSha256, limit });
}

export async function getContentObjectDetail(contentObjectId: string): Promise<ContentObjectDetail> {
  return invoke<ContentObjectDetail>('content_object_detail', { contentObjectId });
}

export async function getFindingsPage(
  code: string | null,
  severity: string | null,
  reviewStatus: string | null,
  category: FindingCategory,
  search: string | null,
  afterId: string | null,
  limit = 100,
): Promise<FindingsPage> {
  return invoke<FindingsPage>('findings_page', {
    request: {
      code,
      severity,
      reviewStatus,
      category,
      search,
      afterId,
      limit,
    },
  });
}

export async function getFindingDetail(findingId: string): Promise<FindingDetail> {
  return invoke<FindingDetail>('get_finding_detail', { findingId });
}

export async function setFindingReviewStatus(
  findingId: string,
  status: ReviewStatus,
  note: string | null,
): Promise<FindingReviewHistory> {
  return invoke<FindingReviewHistory>('set_finding_review_status', { findingId, status, note });
}

export async function clearFindingReviewStatus(
  findingId: string,
  note: string | null,
): Promise<FindingReviewHistory> {
  return invoke<FindingReviewHistory>('clear_finding_review_status', { findingId, note });
}

export async function addFindingReviewNote(
  findingId: string,
  note: string,
): Promise<FindingReviewHistory> {
  return invoke<FindingReviewHistory>('add_finding_review_note', { findingId, note });
}

export async function exportSanitizedSummary(
  runId: string,
  destination: string,
): Promise<string> {
  return invoke<string>('export_sanitized_summary', { runId, destination });
}

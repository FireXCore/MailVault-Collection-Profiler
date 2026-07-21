export type CheckStatus = 'passed' | 'warning' | 'failed';
export type CheckLevel = 'required' | 'recommended' | 'informational';
export type LockState = 'absent' | 'idle' | 'active' | 'indeterminate';
export type AvailabilityState =
  | 'uninspected'
  | 'available'
  | 'missing'
  | 'unreadable'
  | 'invalid_locator'
  | 'non_regular'
  | 'unsafe_reparse_point'
  | 'io_error';
export type SizeState = 'uninspected' | 'match' | 'mismatch' | 'unavailable';

export interface PreflightCheck {
  code: string;
  label: string;
  level: CheckLevel;
  status: CheckStatus;
  detail: string;
}

export interface ArchiveMetrics {
  accounts: number;
  messages: number;
  messageOccurrences: number;
  mimeParts: number;
  attachmentOccurrences: number;
  blobs: number;
  blobBytes: number;
  messageRelations: number;
  participants: number;
}

export interface PreflightReport {
  adapter: string;
  compatible: boolean;
  archiveRoot: string;
  databasePath: string;
  databaseBytes: number;
  archiveIdentity: string | null;
  schemaVersion: number | null;
  journalMode: string | null;
  lockState: LockState;
  metrics: ArchiveMetrics;
  checks: PreflightCheck[];
  warningsCount: number;
  errorsCount: number;
  inspectedAt: string;
}

export type RunStage =
  | 'preflight'
  | 'source_snapshot'
  | 'metadata_inventory'
  | 'reconciliation'
  | 'file_stat'
  | 'fixity'
  | 'format_identification'
  | 'aggregation'
  | 'publish';

export interface ProgressEvent {
  runId: string;
  sequence: number;
  stage: RunStage;
  stageState: 'planned' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
  unit: 'checks' | 'pages' | 'rows' | 'objects' | 'bytes';
  completedItems: number;
  totalItems: number | null;
  completedBytes: number;
  totalBytes: number | null;
  elapsedMs: number;
  instantThroughput: number | null;
  smoothedThroughput: number | null;
  etaMs: number | null;
  activeWorkers: number;
  queueDepth: number;
  warnings: number;
  errors: number;
  currentObjectDisplay: string | null;
  checkpointSequence: number;
}

export interface ErrorReport {
  code: string;
  message: string;
  retryable: boolean;
  context?: Record<string, string>;
}

export interface InventorySummary {
  messages: number;
  messageOccurrences: number;
  participants: number;
  parts: number;
  attachmentOccurrences: number;
  blobRows: number;
  contentObjects: number;
  contentOccurrences: number;
  messageRelations: number;
  zeroByteContentObjects: number;
  sameHashDifferentNames: number;
  sameNameDifferentHashes: number;
}

export interface InventoryResult {
  runId: string;
  sourceSnapshotId: string;
  summary: InventorySummary;
}

export interface FileStatSummary {
  totalObjects: number;
  availableObjects: number;
  missingObjects: number;
  unreadableObjects: number;
  invalidLocatorObjects: number;
  nonRegularObjects: number;
  unsafeReparseObjects: number;
  ioErrorObjects: number;
  sizeMatches: number;
  sizeMismatches: number;
  expectedBytes: number;
  availableBytes: number;
}

export interface FileStatResult {
  runId: string;
  collectionId: string;
  summary: FileStatSummary;
}

export interface SnapshotManifest {
  adapter: string;
  adapterVersion: string;
  runId: string;
  archiveIdentity: string;
  archiveRoot: string;
  sourceDatabase: string;
  snapshotDatabase: string;
  snapshotSha256: string;
  snapshotBytes: number;
  schemaVersion: number;
  sourceMetrics: ArchiveMetrics;
  snapshotMetrics: ArchiveMetrics;
  createdAt: string;
}

export interface SnapshotResult {
  snapshotDirectory: string;
  manifestPath: string;
  manifest: SnapshotManifest;
}

export interface ProfileResult {
  runId: string;
  collectionId: string;
  sourceSnapshotId: string;
  profilerDatabase: string;
  preflight: PreflightReport;
  snapshot: SnapshotResult;
  inventory: InventoryResult;
  fileStat: FileStatResult;
}

export interface InventoryFilters {
  search?: string | undefined;
  availabilityState?: AvailabilityState | undefined;
  sizeState?: SizeState | undefined;
  findingCode?: string | undefined;
}

export interface InventoryPageRequest {
  collectionId: string;
  runId: string;
  filters: InventoryFilters;
  afterSha256?: string;
  limit: number;
}

export interface InventoryObjectRow {
  id: string;
  sha256: string;
  primaryFilename: string;
  sourceDetectedMimeType: string;
  expectedSizeBytes: number;
  actualSizeBytes: number | null;
  occurrenceCount: number;
  filenameVariantCount: number;
  messageCount: number;
  threadCount: number;
  firstSeenAt: string | null;
  lastSeenAt: string | null;
  availabilityState: AvailabilityState;
  sizeState: SizeState;
  findingCount: number;
}

export interface InventoryPage {
  items: InventoryObjectRow[];
  totalFiltered: number;
  nextAfterSha256: string | null;
  hasMore: boolean;
}

export interface FilenameVariantView {
  normalizedFilename: string;
  displayFilename: string;
  occurrenceCount: number;
  firstSeenAt: string | null;
  lastSeenAt: string | null;
}

export interface OccurrenceView {
  occurrenceId: string;
  sourceMessageId: number;
  sourcePartId: number;
  partPath: string;
  filenameOriginal: string | null;
  role: string;
  senderDomain: string | null;
  messageDate: string | null;
  subject: string;
  providerThreadNamespace: string | null;
  providerThreadValue: string | null;
}

export type ReviewStatus =
  | 'acknowledged'
  | 'expected'
  | 'needs_investigation'
  | 'resolved_externally';

export interface FindingView {
  id: string;
  contentObjectId: string | null;
  code: string;
  severity: string;
  message: string;
  evidence: unknown;
  createdAt: string;
  reviewStatus: ReviewStatus | null;
  reviewedAt: string | null;
}

export interface ContentObjectDetail {
  object: InventoryObjectRow;
  filenameVariants: FilenameVariantView[];
  occurrences: OccurrenceView[];
  occurrenceTotal: number;
  occurrencesTruncated: boolean;
  findings: FindingView[];
}

export interface FindingsSummary {
  total: number;
  warnings: number;
  errors: number;
  informational: number;
  zeroByte: number;
  sameHashDifferentNames: number;
  sameNameDifferentHashes: number;
  missing: number;
  sizeMismatch: number;
  invalidLocator: number;
}

export type FindingCategory =
  | 'requires_attention'
  | 'informational_evidence'
  | 'reviewed'
  | 'all';

export interface FindingsPageRequest {
  runId: string;
  code?: string | undefined;
  severity?: string | undefined;
  reviewStatus?: string | undefined;
  category?: FindingCategory | undefined;
  search?: string | undefined;
  afterId?: string | undefined;
  limit: number;
}

export interface FindingsPage {
  items: FindingView[];
  summary: FindingsSummary;
  nextAfterId: string | null;
  hasMore: boolean;
}


export type WorkspaceAccessMode =
  | 'read_write'
  | 'read_only_locked'
  | 'read_only_compatibility';

export type WorkspaceCompatibility =
  | 'compatible'
  | 'migration_required'
  | 'newer_than_application'
  | 'invalid_layout'
  | 'missing_profiler_database'
  | 'corrupted_profiler_database'
  | 'source_workspace_overlap'
  | 'incomplete_migration';

export interface WorkspaceInspection {
  rootPath: string;
  profilerDatabase: string;
  compatibility: WorkspaceCompatibility;
  schemaVersion: number | null;
  supportedSchemaVersion: number;
  migrationRequired: boolean;
  lockActive: boolean;
  runCount: number;
  workspaceId: string | null;
  createdByVersion: string | null;
  lastMigratedByVersion: string | null;
  detail: string;
}

export interface WorkspaceDescriptor {
  workspaceId: string;
  rootPath: string;
  profilerDatabase: string;
  schemaVersion: number;
  createdAt: string;
  createdByVersion: string;
  lastMigratedAt: string | null;
  lastMigratedByVersion: string | null;
  accessMode: WorkspaceAccessMode;
  reviewIntegrityValid: boolean;
}

export type RunCatalogStatus =
  | 'completed'
  | 'failed'
  | 'interrupted'
  | 'cancelled'
  | 'unknown';

export interface ReviewSummary {
  totalFindings: number;
  reviewableFindings: number;
  unreviewed: number;
  acknowledged: number;
  expected: number;
  needsInvestigation: number;
  resolvedExternally: number;
  reviewedFindings: number;
  reviewCompletionPercent: number;
  warningsRemaining: number;
  errorsRemaining: number;
  informationalEvidence: number;
}

export interface RunCatalogEntry {
  runId: string;
  collectionId: string | null;
  sourceSnapshotId: string | null;
  status: RunCatalogStatus;
  persistedState: string;
  startedAt: string;
  completedAt: string | null;
  appVersion: string;
  archiveFingerprint: string | null;
  sourceSchemaVersion: number | null;
  messages: number;
  mimeParts: number;
  blobs: number;
  findings: number;
  errors: number;
  warnings: number;
  reviewSummary: ReviewSummary;
}

export interface OpenWorkspaceResult {
  descriptor: WorkspaceDescriptor;
  runs: RunCatalogEntry[];
}

export interface ActiveRunContext {
  run: RunCatalogEntry;
  collectionId: string;
  sourceSnapshotId: string;
  inventory: InventorySummary;
  findings: FindingsSummary;
}

export type ReviewAction = 'status_set' | 'status_cleared' | 'note_added';
export type ReviewActorKind = 'local_interactive_user' | 'local_cli_user';

export interface FindingReviewEvent {
  eventId: string;
  runId: string;
  findingId: string;
  sequence: number;
  action: ReviewAction;
  previousStatus: ReviewStatus | null;
  newStatus: ReviewStatus | null;
  note: string | null;
  actorKind: ReviewActorKind;
  actorLabel: string | null;
  occurredAt: string;
  previousEventHash: string | null;
  eventHash: string;
}

export interface FindingReviewHistory {
  findingId: string;
  currentStatus: ReviewStatus | null;
  latestNote: string | null;
  integrityValid: boolean;
  events: FindingReviewEvent[];
}

export interface FindingDetail {
  finding: FindingView;
  object: ContentObjectDetail | null;
  review: FindingReviewHistory;
}

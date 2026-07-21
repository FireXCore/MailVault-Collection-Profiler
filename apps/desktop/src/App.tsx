import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  addFindingReviewNote,
  chooseDirectory,
  chooseExportPath,
  clearFindingReviewStatus,
  closeWorkspace,
  exportSanitizedSummary,
  getContentObjectDetail,
  getCurrentWorkspace,
  getFindingDetail,
  getFindingsPage,
  getFormatPage,
  getFormatSummary,
  getInventoryPage,
  getWorkspaceRuns,
  identifyFormats,
  inspectWorkspace,
  normalizeError,
  openExistingRun,
  openWorkspace,
  preflightArchive,
  probeFormatTool,
  profileCollection,
  setFindingReviewStatus,
} from './api';
import FormatsView from './FormatsView';
import type {
  ActiveRunContext,
  AvailabilityState,
  ContentObjectDetail,
  ErrorReport,
  FindingCategory,
  FindingDetail,
  FindingView,
  FindingsPage,
  FormatPage,
  FormatState,
  FormatSummary,
  FormatToolIdentity,
  InventoryFilters,
  InventoryObjectRow,
  InventoryPage,
  OpenWorkspaceResult,
  PreflightReport,
  ProgressEvent,
  ReviewStatus,
  RunCatalogEntry,
  SizeState,
  WorkspaceDescriptor,
  WorkspaceInspection,
} from './types';

const number = new Intl.NumberFormat('en-US');
const decimal = new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 });
const dateTime = new Intl.DateTimeFormat('en-US', {
  year: 'numeric',
  month: 'short',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
});
const PAGE_SIZE = 100;

type View = 'setup' | 'runs' | 'inventory' | 'formats' | 'findings';
type BusyState =
  | 'preflight'
  | 'profile'
  | 'workspace'
  | 'runs'
  | 'inventory'
  | 'findings'
  | 'formats'
  | 'format_tool'
  | 'format_run'
  | 'detail'
  | 'review'
  | 'export'
  | null;

function formatBytes(value: number): string {
  if (value < 1024) return `${number.format(value)} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB'] as const;
  let size = value / 1024;
  let unit: string = units[0];
  for (let index = 1; index < units.length && size >= 1024; index += 1) {
    size /= 1024;
    unit = units[index] ?? unit;
  }
  return `${decimal.format(size)} ${unit}`;
}

function formatDate(value: string | null | undefined): string {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : dateTime.format(date);
}

function shortHash(value: string | null | undefined): string {
  if (!value) return '—';
  return value.length > 16 ? `${value.slice(0, 12)}…${value.slice(-4)}` : value;
}

function titleCase(value: string): string {
  return value.replaceAll('_', ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function StateBadge({ value }: { value: string | null | undefined }) {
  const normalized = value ?? 'unreviewed';
  return <span className={`state-badge state-${normalized}`}>{titleCase(normalized)}</span>;
}

function PathPicker({
  label,
  value,
  title,
  onChange,
}: {
  label: string;
  value: string;
  title: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="path-field">
      <span>{label}</span>
      <div>
        <input value={value} onChange={(event) => onChange(event.target.value)} />
        <button
          type="button"
          onClick={() => {
            void chooseDirectory(title).then((selected) => {
              if (selected) onChange(selected);
            });
          }}
        >
          Browse
        </button>
      </div>
    </label>
  );
}

function ProgressCard({ progress }: { progress: ProgressEvent | null }) {
  if (!progress) return null;
  const percent = progress.totalItems
    ? Math.min(100, (progress.completedItems / progress.totalItems) * 100)
    : 0;
  return (
    <section className="pipeline-card">
      <div className="pipeline-title">
        <div>
          <span className="section-index">ACTIVE PIPELINE</span>
          <h3>{titleCase(progress.stage)}</h3>
        </div>
        <strong>{decimal.format(percent)}%</strong>
      </div>
      <div className="progress-track"><span style={{ width: `${percent}%` }} /></div>
      <dl className="pipeline-metrics">
        <div><dt>Objects</dt><dd>{number.format(progress.completedItems)} / {number.format(progress.totalItems ?? 0)}</dd></div>
        <div><dt>Throughput</dt><dd>{progress.smoothedThroughput ? `${number.format(Math.round(progress.smoothedThroughput))}/s` : '—'}</dd></div>
        <div><dt>Elapsed</dt><dd>{decimal.format(progress.elapsedMs / 1000)}s</dd></div>
        <div><dt>Workers</dt><dd>{progress.activeWorkers}</dd></div>
        <div><dt>Warnings / Errors</dt><dd>{progress.warnings} / {progress.errors}</dd></div>
      </dl>
    </section>
  );
}

function SetupView({
  archiveRoot,
  workspaceRoot,
  workspaceToOpen,
  preflight,
  workspaceInspection,
  progress,
  busy,
  onArchiveRoot,
  onWorkspaceRoot,
  onWorkspaceToOpen,
  onPreflight,
  onProfile,
  onInspectWorkspace,
  onOpenWorkspace,
}: {
  archiveRoot: string;
  workspaceRoot: string;
  workspaceToOpen: string;
  preflight: PreflightReport | null;
  workspaceInspection: WorkspaceInspection | null;
  progress: ProgressEvent | null;
  busy: BusyState;
  onArchiveRoot: (value: string) => void;
  onWorkspaceRoot: (value: string) => void;
  onWorkspaceToOpen: (value: string) => void;
  onPreflight: () => void;
  onProfile: () => void;
  onInspectWorkspace: () => void;
  onOpenWorkspace: (readOnly: boolean, allowMigration: boolean) => void;
}) {
  const compatible = preflight?.compatible === true;
  return (
    <>
      <header className="topbar">
        <div><span className="eyebrow">START</span><h1>Profile a new archive or reopen a workspace.</h1></div>
        <div className="source-pill"><span />Local only · source read-only</div>
      </header>
      <section className="journey-grid">
        <article className="journey-card">
          <span className="section-index">NEW PROFILE</span>
          <h2>Profile new archive</h2>
          <p>Validate the MailVault source, create a consistent snapshot, and build a separate profiler workspace.</p>
          <PathPicker label="MailVault archive root" value={archiveRoot} title="Select MailVault archive" onChange={onArchiveRoot} />
          <button className="primary-action" type="button" disabled={!archiveRoot.trim() || busy !== null} onClick={onPreflight}>
            {busy === 'preflight' ? 'Running preflight…' : 'Run read-only preflight'}
          </button>
          {preflight ? (
            <div className={`compatibility-result ${compatible ? 'result-ok' : 'result-failed'}`}>
              <strong>{compatible ? 'Source contract is compatible' : 'Preflight failed'}</strong>
              <span>Schema {preflight.schemaVersion ?? 'unknown'} · {number.format(preflight.metrics.messages)} messages · {number.format(preflight.metrics.blobs)} blobs</span>
            </div>
          ) : null}
          <PathPicker label="Profiler workspace" value={workspaceRoot} title="Select profiler workspace" onChange={onWorkspaceRoot} />
          <button className="primary-action" type="button" disabled={!compatible || !workspaceRoot.trim() || busy !== null} onClick={onProfile}>
            {busy === 'profile' ? 'Profiling collection…' : 'Create physical inventory'}
          </button>
        </article>

        <article className="journey-card">
          <span className="section-index">EXISTING WORKSPACE</span>
          <h2>Open existing workspace</h2>
          <p>Reopen completed or interrupted runs without profiling the source archive again.</p>
          <PathPicker label="Workspace directory" value={workspaceToOpen} title="Select existing profiler workspace" onChange={onWorkspaceToOpen} />
          <button className="secondary-action" type="button" disabled={!workspaceToOpen.trim() || busy !== null} onClick={onInspectWorkspace}>
            {busy === 'workspace' ? 'Inspecting workspace…' : 'Inspect workspace'}
          </button>
          {workspaceInspection ? (
            <div className={`workspace-inspection compatibility-${workspaceInspection.compatibility}`}>
              <div><span>Compatibility</span><strong>{titleCase(workspaceInspection.compatibility)}</strong></div>
              <div><span>Schema</span><strong>{workspaceInspection.schemaVersion ?? '—'} / {workspaceInspection.supportedSchemaVersion}</strong></div>
              <div><span>Runs</span><strong>{number.format(workspaceInspection.runCount)}</strong></div>
              <div><span>Writer lock</span><strong>{workspaceInspection.lockActive ? 'Active' : 'Available'}</strong></div>
              <p>{workspaceInspection.detail}</p>
            </div>
          ) : null}
          <div className="button-row">
            <button
              className="primary-action"
              type="button"
              disabled={!workspaceInspection || !['compatible', 'migration_required'].includes(workspaceInspection.compatibility) || busy !== null}
              onClick={() => onOpenWorkspace(false, workspaceInspection?.migrationRequired ?? false)}
            >
              {workspaceInspection?.migrationRequired ? 'Back up, migrate & open' : 'Open workspace'}
            </button>
            <button
              className="secondary-action"
              type="button"
              disabled={!workspaceInspection || workspaceInspection.compatibility !== 'compatible' || busy !== null}
              onClick={() => onOpenWorkspace(true, false)}
            >
              Open read-only
            </button>
          </div>
        </article>
      </section>
      <ProgressCard progress={progress} />
    </>
  );
}

function RunsView({
  descriptor,
  runs,
  loading,
  onOpen,
}: {
  descriptor: WorkspaceDescriptor;
  runs: RunCatalogEntry[];
  loading: boolean;
  onOpen: (runId: string) => void;
}) {
  return (
    <>
      <header className="topbar">
        <div><span className="eyebrow">WORKSPACE</span><h1>Previous profiling runs</h1></div>
        <StateBadge value={descriptor.accessMode} />
      </header>
      <section className="workspace-banner">
        <div><span>Workspace schema</span><strong>v{descriptor.schemaVersion}</strong></div>
        <div><span>Created by</span><strong>{descriptor.createdByVersion}</strong></div>
        <div><span>Review integrity</span><strong>{descriptor.reviewIntegrityValid ? 'Valid' : 'Failed'}</strong></div>
        <div><span>Runs</span><strong>{number.format(runs.length)}</strong></div>
      </section>
      <section className="inventory-shell">
        <div className="table-wrap">
          <table className="inventory-table run-table">
            <thead><tr><th>Started</th><th>Status</th><th>Version</th><th>Messages</th><th>MIME parts</th><th>Binaries</th><th>Findings</th><th>Review</th><th /></tr></thead>
            <tbody>
              {runs.map((run) => (
                <tr key={run.runId}>
                  <td><strong>{formatDate(run.startedAt)}</strong><small>{shortHash(run.runId)}</small></td>
                  <td><StateBadge value={run.status} /></td>
                  <td>{run.appVersion}</td>
                  <td>{number.format(run.messages)}</td>
                  <td>{number.format(run.mimeParts)}</td>
                  <td>{number.format(run.blobs)}</td>
                  <td><strong>{number.format(run.findings)}</strong><small>{run.errors} errors · {run.warnings} warnings</small></td>
                  <td><strong>{run.reviewSummary.reviewCompletionPercent}%</strong><small>{number.format(run.reviewSummary.reviewedFindings)} reviewed</small></td>
                  <td><button className="table-action" type="button" disabled={loading || !run.collectionId || !run.sourceSnapshotId} onClick={() => onOpen(run.runId)}>Open run</button></td>
                </tr>
              ))}
              {!loading && runs.length === 0 ? <tr><td className="table-empty" colSpan={9}>No profiler runs exist in this workspace.</td></tr> : null}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}

function InventoryView({
  activeRun,
  page,
  filters,
  loading,
  canGoBack,
  onFilters,
  onSearch,
  onNext,
  onPrevious,
  onSelect,
}: {
  activeRun: ActiveRunContext;
  page: InventoryPage | null;
  filters: InventoryFilters;
  loading: boolean;
  canGoBack: boolean;
  onFilters: (filters: InventoryFilters) => void;
  onSearch: () => void;
  onNext: () => void;
  onPrevious: () => void;
  onSelect: (row: InventoryObjectRow) => void;
}) {
  return (
    <>
      <header className="topbar"><div><span className="eyebrow">PHYSICAL INVENTORY</span><h1>Reopened physical inventory</h1></div><div className="source-pill"><span />Run {shortHash(activeRun.run.runId)}</div></header>
      <section className="inventory-overview">
        <article><span>Messages</span><strong>{number.format(activeRun.inventory.messages)}</strong><small>source records</small></article>
        <article><span>MIME parts</span><strong>{number.format(activeRun.inventory.parts)}</strong><small>parsed structure</small></article>
        <article><span>Physical binaries</span><strong>{number.format(activeRun.inventory.contentObjects)}</strong><small>content-addressed objects</small></article>
        <article><span>Findings</span><strong>{number.format(activeRun.findings.total)}</strong><small>{activeRun.findings.errors} errors · {activeRun.findings.warnings} warnings</small></article>
      </section>
      <section className="inventory-shell">
        <div className="filter-bar">
          <input placeholder="Hash, filename, MIME type, sender domain, subject…" value={filters.search ?? ''} onChange={(event) => onFilters({ ...filters, search: event.target.value || undefined })} />
          <select value={filters.availabilityState ?? ''} onChange={(event) => onFilters({ ...filters, availabilityState: (event.target.value || undefined) as AvailabilityState | undefined })}><option value="">All availability</option><option value="available">Available</option><option value="missing">Missing</option><option value="unreadable">Unreadable</option><option value="invalid_locator">Invalid locator</option></select>
          <select value={filters.sizeState ?? ''} onChange={(event) => onFilters({ ...filters, sizeState: (event.target.value || undefined) as SizeState | undefined })}><option value="">All sizes</option><option value="match">Size match</option><option value="mismatch">Size mismatch</option><option value="unavailable">Unavailable</option></select>
          <button className="filter-action" type="button" onClick={onSearch}>Apply</button>
        </div>
        <div className="table-wrap">
          <table className="inventory-table">
            <thead><tr><th>Content object</th><th>Type</th><th>Expected / actual</th><th>Occurrences</th><th>Messages</th><th>Physical state</th><th>Findings</th></tr></thead>
            <tbody>
              {page?.items.map((row) => (
                <tr key={row.id} onClick={() => onSelect(row)}>
                  <td><strong>{row.primaryFilename}</strong><code>{shortHash(row.sha256)}</code></td>
                  <td>{row.sourceDetectedMimeType}</td>
                  <td>{formatBytes(row.expectedSizeBytes)}<small>{row.actualSizeBytes === null ? 'not available' : formatBytes(row.actualSizeBytes)}</small></td>
                  <td>{number.format(row.occurrenceCount)}</td>
                  <td>{number.format(row.messageCount)}<small>{number.format(row.threadCount)} threads</small></td>
                  <td><StateBadge value={row.availabilityState} /><StateBadge value={row.sizeState} /></td>
                  <td>{row.findingCount > 0 ? <span className="finding-count">{row.findingCount}</span> : '—'}</td>
                </tr>
              ))}
              {!loading && page?.items.length === 0 ? <tr><td className="table-empty" colSpan={7}>No physical objects match these filters.</td></tr> : null}
            </tbody>
          </table>
        </div>
        <div className="pager"><button type="button" onClick={onPrevious} disabled={!canGoBack || loading}>Previous</button><span>{loading ? 'Loading…' : `${number.format(page?.totalFiltered ?? 0)} matching objects`}</span><button type="button" onClick={onNext} disabled={!page?.hasMore || loading}>Next {PAGE_SIZE}</button></div>
      </section>
    </>
  );
}

function FindingsView({
  activeRun,
  page,
  loading,
  category,
  severity,
  code,
  reviewStatus,
  search,
  onCategory,
  onSeverity,
  onCode,
  onReviewStatus,
  onSearch,
  onReload,
  onNext,
  onSelect,
  onExport,
}: {
  activeRun: ActiveRunContext;
  page: FindingsPage | null;
  loading: boolean;
  category: FindingCategory;
  severity: string;
  code: string;
  reviewStatus: string;
  search: string;
  onCategory: (value: FindingCategory) => void;
  onSeverity: (value: string) => void;
  onCode: (value: string) => void;
  onReviewStatus: (value: string) => void;
  onSearch: (value: string) => void;
  onReload: () => void;
  onNext: () => void;
  onSelect: (finding: FindingView) => void;
  onExport: (extension: 'json' | 'csv') => void;
}) {
  const summary = activeRun.run.reviewSummary;
  return (
    <>
      <header className="topbar"><div><span className="eyebrow">FINDINGS REVIEW</span><h1>Review evidence without changing MailVault.</h1></div><div className="button-row"><button className="secondary-action compact" type="button" onClick={() => onExport('json')}>Export JSON summary</button><button className="secondary-action compact" type="button" onClick={() => onExport('csv')}>Export sanitized CSV</button></div></header>
      <section className="inventory-overview findings-overview">
        <article><span>Reviewable</span><strong>{number.format(summary.reviewableFindings)}</strong><small>errors and warnings</small></article>
        <article><span>Unreviewed</span><strong>{number.format(summary.unreviewed)}</strong><small>all finding classes</small></article>
        <article><span>Needs investigation</span><strong>{number.format(summary.needsInvestigation)}</strong><small>manual follow-up</small></article>
        <article><span>Review completion</span><strong>{summary.reviewCompletionPercent}%</strong><small>{number.format(summary.reviewedFindings)} decisions recorded</small></article>
      </section>
      <section className="inventory-shell">
        <div className="finding-tabs">
          {([
            ['requires_attention', 'Requires attention'],
            ['informational_evidence', 'Informational evidence'],
            ['reviewed', 'Reviewed'],
            ['all', 'All findings'],
          ] as const).map(([value, label]) => <button type="button" className={category === value ? 'tab-active' : ''} key={value} onClick={() => onCategory(value)}>{label}</button>)}
        </div>
        <div className="filter-bar findings-filter">
          <input placeholder="Search code or technical explanation…" value={search} onChange={(event) => onSearch(event.target.value)} />
          <select value={severity} onChange={(event) => onSeverity(event.target.value)}><option value="">All severities</option><option value="error">Errors</option><option value="warning">Warnings</option><option value="info">Informational</option></select>
          <select value={reviewStatus} onChange={(event) => onReviewStatus(event.target.value)}><option value="">All review states</option><option value="unreviewed">Unreviewed</option><option value="acknowledged">Acknowledged</option><option value="expected">Expected</option><option value="needs_investigation">Needs investigation</option><option value="resolved_externally">Resolved externally</option></select>
          <input className="code-filter" placeholder="Finding code" value={code} onChange={(event) => onCode(event.target.value)} />
          <button className="filter-action" type="button" onClick={onReload}>Apply</button>
        </div>
        <div className="finding-list">
          {page?.items.map((finding) => (
            <button className="finding-row" type="button" key={finding.id} onClick={() => onSelect(finding)}>
              <StateBadge value={finding.severity} />
              <div><strong>{titleCase(finding.code)}</strong><p>{finding.message}</p></div>
              <StateBadge value={finding.reviewStatus} />
              <time>{formatDate(finding.reviewedAt ?? finding.createdAt)}</time>
              <span>Review →</span>
            </button>
          ))}
          {!loading && page?.items.length === 0 ? <div className="table-empty">No findings match these filters.</div> : null}
        </div>
        <div className="pager"><span>{loading ? 'Loading findings…' : `${number.format(page?.items.length ?? 0)} loaded`}</span><button type="button" onClick={onNext} disabled={!page?.hasMore || loading}>Load next</button></div>
      </section>
    </>
  );
}

function ObjectDrawer({ detail, onClose }: { detail: ContentObjectDetail; onClose: () => void }) {
  return (
    <div className="drawer-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <aside className="detail-drawer" aria-label="Content object detail">
        <div className="drawer-head"><div><span className="section-index">CONTENT OBJECT</span><h2>{detail.object.primaryFilename}</h2></div><button type="button" onClick={onClose}>×</button></div>
        <div className="drawer-body">
          <section className="identity-card"><code>{detail.object.sha256}</code><div><StateBadge value={detail.object.availabilityState} /><StateBadge value={detail.object.sizeState} /></div><dl><div><dt>Expected size</dt><dd>{formatBytes(detail.object.expectedSizeBytes)}</dd></div><div><dt>Occurrences</dt><dd>{number.format(detail.occurrenceTotal)}</dd></div><div><dt>Messages</dt><dd>{number.format(detail.object.messageCount)}</dd></div><div><dt>Threads</dt><dd>{number.format(detail.object.threadCount)}</dd></div></dl></section>
          <section className="drawer-section"><div className="drawer-section-head"><h3>Filename history</h3><span>{detail.filenameVariants.length}</span></div><div className="variant-list">{detail.filenameVariants.map((variant) => <article key={variant.normalizedFilename}><strong>{variant.displayFilename}</strong><span>{number.format(variant.occurrenceCount)}×</span><small>{formatDate(variant.firstSeenAt)} → {formatDate(variant.lastSeenAt)}</small></article>)}</div></section>
          <section className="drawer-section"><div className="drawer-section-head"><h3>Email occurrences</h3><span>{number.format(detail.occurrenceTotal)}</span></div><div className="occurrence-list">{detail.occurrences.map((occurrence) => <article key={occurrence.occurrenceId}><div><strong>{occurrence.subject || '(no subject)'}</strong><p>{occurrence.senderDomain ?? 'unknown sender'} · {formatDate(occurrence.messageDate)}</p></div><div><span>{occurrence.filenameOriginal ?? '[unnamed]'}</span><code>{occurrence.partPath}</code></div></article>)}</div></section>
        </div>
      </aside>
    </div>
  );
}

function FindingReviewDrawer({
  detail,
  canWrite,
  busy,
  onClose,
  onSetStatus,
  onClear,
  onAddNote,
}: {
  detail: FindingDetail;
  canWrite: boolean;
  busy: boolean;
  onClose: () => void;
  onSetStatus: (status: ReviewStatus, note: string) => void;
  onClear: (note: string) => void;
  onAddNote: (note: string) => void;
}) {
  const [status, setStatus] = useState<ReviewStatus>(detail.review.currentStatus ?? 'acknowledged');
  const [note, setNote] = useState('');
  const noteRequired = status === 'needs_investigation' || status === 'resolved_externally';
  return (
    <div className="drawer-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <aside className="detail-drawer review-drawer" aria-label="Finding review">
        <div className="drawer-head"><div><span className="section-index">FINDING REVIEW</span><h2>{titleCase(detail.finding.code)}</h2></div><button type="button" onClick={onClose}>×</button></div>
        <div className="drawer-body">
          <section className="identity-card"><div><StateBadge value={detail.finding.severity} /><StateBadge value={detail.review.currentStatus} /></div><p>{detail.finding.message}</p><dl><div><dt>Finding token</dt><dd>{shortHash(detail.finding.id)}</dd></div><div><dt>Created</dt><dd>{formatDate(detail.finding.createdAt)}</dd></div><div><dt>Integrity</dt><dd>{detail.review.integrityValid ? 'Valid' : 'Failed'}</dd></div><div><dt>Events</dt><dd>{detail.review.events.length}</dd></div></dl></section>
          {detail.object ? <section className="drawer-section"><div className="drawer-section-head"><h3>Related content object</h3></div><div className="object-summary"><strong>{detail.object.object.primaryFilename}</strong><code>{detail.object.object.sha256}</code><span>{number.format(detail.object.occurrenceTotal)} occurrences</span></div></section> : null}
          <section className="drawer-section review-controls"><div className="drawer-section-head"><h3>Review decision</h3><span>{canWrite ? 'Append-only' : 'Read-only'}</span></div><select value={status} disabled={!canWrite || busy} onChange={(event) => setStatus(event.target.value as ReviewStatus)}><option value="acknowledged">Acknowledged</option><option value="expected">Expected</option><option value="needs_investigation">Needs investigation</option><option value="resolved_externally">Resolved externally</option></select><textarea value={note} disabled={!canWrite || busy} maxLength={4000} placeholder={noteRequired ? 'A note is required for this status.' : 'Optional review note'} onChange={(event) => setNote(event.target.value)} /><div className="button-row"><button className="primary-action" type="button" disabled={!canWrite || busy || (noteRequired && !note.trim())} onClick={() => onSetStatus(status, note)}>Record decision</button><button className="secondary-action" type="button" disabled={!canWrite || busy || !note.trim()} onClick={() => onAddNote(note)}>Add note only</button><button className="danger-action" type="button" disabled={!canWrite || busy || detail.review.currentStatus === null} onClick={() => onClear(note)}>Clear status</button></div></section>
          <section className="drawer-section"><div className="drawer-section-head"><h3>Append-only review history</h3><span>{detail.review.events.length}</span></div><div className="review-timeline">{detail.review.events.map((event) => <article key={event.eventId}><span className="timeline-dot" /><div><strong>{titleCase(event.action)}</strong><p>{event.previousStatus ?? 'unreviewed'} → {event.newStatus ?? 'unreviewed'}</p>{event.note ? <blockquote>{event.note}</blockquote> : null}<small>{formatDate(event.occurredAt)} · {titleCase(event.actorKind)} · hash {shortHash(event.eventHash)}</small></div></article>)}{detail.review.events.length === 0 ? <p className="table-empty">No review decisions have been recorded.</p> : null}</div></section>
        </div>
      </aside>
    </div>
  );
}

function App() {
  const [view, setView] = useState<View>('setup');
  const [archiveRoot, setArchiveRoot] = useState('');
  const [workspaceRoot, setWorkspaceRoot] = useState('');
  const [workspaceToOpen, setWorkspaceToOpen] = useState('');
  const [preflight, setPreflight] = useState<PreflightReport | null>(null);
  const [workspaceInspection, setWorkspaceInspection] = useState<WorkspaceInspection | null>(null);
  const [workspace, setWorkspace] = useState<OpenWorkspaceResult | null>(null);
  const [runs, setRuns] = useState<RunCatalogEntry[]>([]);
  const [activeRun, setActiveRun] = useState<ActiveRunContext | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [error, setError] = useState<ErrorReport | null>(null);
  const [busy, setBusy] = useState<BusyState>(null);

  const [inventoryPage, setInventoryPage] = useState<InventoryPage | null>(null);
  const [inventoryFilters, setInventoryFilters] = useState<InventoryFilters>({});
  const [appliedInventoryFilters, setAppliedInventoryFilters] = useState<InventoryFilters>({});
  const [inventoryAfter, setInventoryAfter] = useState<string | undefined>();
  const [inventoryHistory, setInventoryHistory] = useState<Array<string | undefined>>([]);
  const [inventoryVersion, setInventoryVersion] = useState(0);
  const [objectDetail, setObjectDetail] = useState<ContentObjectDetail | null>(null);

  const [findingsPage, setFindingsPage] = useState<FindingsPage | null>(null);
  const [findingCategory, setFindingCategory] = useState<FindingCategory>('requires_attention');
  const [findingSeverity, setFindingSeverity] = useState('');
  const [findingCode, setFindingCode] = useState('');
  const [findingReviewStatus, setFindingReviewStatusFilter] = useState('');
  const [findingSearch, setFindingSearch] = useState('');
  const [appliedFindingFilters, setAppliedFindingFilters] = useState({ category: 'requires_attention' as FindingCategory, severity: '', code: '', reviewStatus: '', search: '' });
  const [findingAfter, setFindingAfter] = useState<string | undefined>();
  const [findingsVersion, setFindingsVersion] = useState(0);
  const [findingDetail, setFindingDetail] = useState<FindingDetail | null>(null);

  const [formatSummary, setFormatSummary] = useState<FormatSummary | null>(null);
  const [formatPage, setFormatPage] = useState<FormatPage | null>(null);
  const [formatTool, setFormatTool] = useState<FormatToolIdentity | null>(null);
  const [formatSearch, setFormatSearch] = useState('');
  const [formatState, setFormatState] = useState<FormatState | ''>('');
  const [formatPuid, setFormatPuid] = useState('');
  const [formatMismatchOnly, setFormatMismatchOnly] = useState(false);
  const [appliedFormatFilters, setAppliedFormatFilters] = useState({ search: '', state: '' as FormatState | '', puid: '', mismatchOnly: false });
  const [formatAfter, setFormatAfter] = useState<string | undefined>();
  const [formatVersion, setFormatVersion] = useState(0);

  const canReview = workspace?.descriptor.accessMode === 'read_write' && workspace.descriptor.reviewIntegrityValid;

  async function runPreflight() {
    if (!archiveRoot.trim()) return;
    setBusy('preflight'); setError(null); setPreflight(null); setProgress(null);
    try { setPreflight(await preflightArchive(archiveRoot.trim())); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function createProfile() {
    if (!preflight?.compatible || !workspaceRoot.trim()) return;
    setBusy('profile'); setError(null); setProgress(null);
    try {
      const result = await profileCollection(archiveRoot.trim(), workspaceRoot.trim(), setProgress);
      const currentWorkspace = await getCurrentWorkspace();
      const opened = await openExistingRun(result.runId);
      setWorkspace(currentWorkspace);
      setRuns(currentWorkspace.runs); setActiveRun(opened); resetRunViews(); setView('inventory');
    } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function inspectExistingWorkspace() {
    if (!workspaceToOpen.trim()) return;
    setBusy('workspace'); setError(null); setWorkspaceInspection(null);
    try { setWorkspaceInspection(await inspectWorkspace(workspaceToOpen.trim())); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function openExistingWorkspace(readOnly: boolean, allowMigration: boolean) {
    if (!workspaceToOpen.trim()) return;
    if (allowMigration) {
      const currentSchema = workspaceInspection?.schemaVersion ?? 'unknown';
      const requiredSchema = workspaceInspection?.supportedSchemaVersion ?? 'unknown';
      const confirmed = window.confirm(
        `This workspace must be migrated from schema ${currentSchema} to ${requiredSchema}.\n\n` +
        'A SQLite backup will be created before migration. The MailVault source archive will not be modified.'
      );
      if (!confirmed) return;
    }
    setBusy('workspace'); setError(null);
    try {
      const opened = await openWorkspace(workspaceToOpen.trim(), readOnly, allowMigration);
      setWorkspace(opened); setRuns(opened.runs); setActiveRun(null); resetRunViews(); setView('runs');
    } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function refreshRuns() {
    if (!workspace) return;
    setBusy('runs');
    try { const next = await getWorkspaceRuns(); setRuns(next); setWorkspace({ ...workspace, runs: next }); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function activateRun(runId: string) {
    setBusy('runs'); setError(null);
    try { setActiveRun(await openExistingRun(runId)); resetRunViews(); setView('inventory'); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  function resetRunViews() {
    setInventoryPage(null); setInventoryFilters({}); setAppliedInventoryFilters({}); setInventoryAfter(undefined); setInventoryHistory([]); setInventoryVersion((value) => value + 1); setFindingsPage(null); setFindingAfter(undefined); setFindingDetail(null); setObjectDetail(null); setFindingsVersion((value) => value + 1); setFormatSummary(null); setFormatPage(null); setFormatTool(null); setFormatAfter(undefined); setFormatVersion((value) => value + 1);
  }

  const loadInventory = useCallback(async () => {
    if (!activeRun) return;
    setBusy('inventory'); setError(null);
    try { setInventoryPage(await getInventoryPage(appliedInventoryFilters, inventoryAfter ?? null, PAGE_SIZE)); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }, [activeRun, appliedInventoryFilters, inventoryAfter]);

  const loadFindings = useCallback(async () => {
    if (!activeRun) return;
    setBusy('findings'); setError(null);
    try { setFindingsPage(await getFindingsPage(appliedFindingFilters.code || null, appliedFindingFilters.severity || null, appliedFindingFilters.reviewStatus || null, appliedFindingFilters.category, appliedFindingFilters.search || null, findingAfter ?? null, PAGE_SIZE)); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }, [activeRun, appliedFindingFilters, findingAfter]);

  const loadFormats = useCallback(async () => {
    if (!activeRun) return;
    setBusy('formats'); setError(null);
    try {
      const [summary, page] = await Promise.all([
        getFormatSummary(),
        getFormatPage(
          appliedFormatFilters.search || null,
          appliedFormatFilters.state || null,
          appliedFormatFilters.puid || null,
          appliedFormatFilters.mismatchOnly,
          formatAfter ?? null,
          PAGE_SIZE,
        ),
      ]);
      setFormatSummary(summary);
      setFormatPage(page);
      if (summary.tool) setFormatTool(summary.tool);
    } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }, [activeRun, appliedFormatFilters, formatAfter]);

  async function probeFormats() {
    setBusy('format_tool'); setError(null);
    try { setFormatTool(await probeFormatTool()); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function runFormats() {
    if (!activeRun || !canReview) return;
    setBusy('format_run'); setError(null); setProgress(null);
    try {
      await identifyFormats({ batchSize: 2048, workers: 0, timeoutSeconds: 900, resume: true }, setProgress);
      setFormatVersion((value) => value + 1);
      setFormatAfter(undefined);
      await loadFormats();
    } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  useEffect(() => { if (view === 'inventory' && activeRun) void loadInventory(); }, [view, activeRun, inventoryVersion, loadInventory]);
  useEffect(() => { if (view === 'findings' && activeRun) void loadFindings(); }, [view, activeRun, findingsVersion, loadFindings]);
  useEffect(() => { if (view === 'formats' && activeRun) void loadFormats(); }, [view, activeRun, formatVersion, loadFormats]);

  async function openObject(row: InventoryObjectRow) {
    setBusy('detail'); setError(null);
    try { setObjectDetail(await getContentObjectDetail(row.id)); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function openFinding(finding: FindingView) {
    setBusy('detail'); setError(null);
    try { setFindingDetail(await getFindingDetail(finding.id)); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function updateReview(action: () => Promise<unknown>) {
    setBusy('review'); setError(null);
    try {
      await action();
      if (findingDetail) setFindingDetail(await getFindingDetail(findingDetail.finding.id));
      if (activeRun) setActiveRun(await openExistingRun(activeRun.run.runId));
      await refreshRuns();
      setFindingsVersion((value) => value + 1);
    } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  async function exportRun(extension: 'json' | 'csv') {
    if (!activeRun) return;
    const destination = await chooseExportPath('Export sanitized review data', `mailvault-profile-${shortHash(activeRun.run.runId)}-sanitized.${extension}`, extension);
    if (!destination) return;
    setBusy('export'); setError(null);
    try { await exportSanitizedSummary(activeRun.run.runId, destination); } catch (caught) { setError(normalizeError(caught)); } finally { setBusy(null); }
  }

  const navigation = useMemo(() => [
    { id: 'setup' as const, index: '01', label: 'Start', enabled: true },
    { id: 'runs' as const, index: '02', label: 'Workspace runs', enabled: workspace !== null },
    { id: 'inventory' as const, index: '03', label: 'Physical inventory', enabled: activeRun !== null },
    { id: 'formats' as const, index: '04', label: 'Exact formats', enabled: activeRun !== null },
    { id: 'findings' as const, index: '05', label: 'Findings review', enabled: activeRun !== null },
  ], [workspace, activeRun]);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-mark" aria-hidden="true"><span /><span /><span /></div>
        <div className="brand-copy"><strong>MailVault</strong><span>Collection Profiler</span></div>
        <nav aria-label="Primary">{navigation.map((item) => <button className={`nav-item ${view === item.id ? 'nav-active' : ''}`} type="button" disabled={!item.enabled} key={item.id} onClick={() => setView(item.id)}><span className="nav-icon">{item.index}</span>{item.label}</button>)}</nav>
        {activeRun ? <div className="collection-mini"><span>Active run</span><strong>{number.format(activeRun.inventory.contentObjects)} binaries</strong><small>{shortHash(activeRun.run.runId)}</small></div> : null}
        {workspace ? <button className="close-workspace" type="button" onClick={() => { void closeWorkspace().then(() => { setWorkspace(null); setRuns([]); setActiveRun(null); setView('setup'); }); }}>Close workspace</button> : null}
        <div className="sidebar-foot"><span className="local-indicator" />Local only · Read-only source</div>
      </aside>
      <main>
        {error ? <section className="error-panel global-error" role="alert"><div><span>{error.code}</span><strong>{error.message}</strong></div><button type="button" onClick={() => setError(null)}>Dismiss</button></section> : null}
        {view === 'setup' ? <SetupView archiveRoot={archiveRoot} workspaceRoot={workspaceRoot} workspaceToOpen={workspaceToOpen} preflight={preflight} workspaceInspection={workspaceInspection} progress={progress} busy={busy} onArchiveRoot={setArchiveRoot} onWorkspaceRoot={setWorkspaceRoot} onWorkspaceToOpen={setWorkspaceToOpen} onPreflight={() => void runPreflight()} onProfile={() => void createProfile()} onInspectWorkspace={() => void inspectExistingWorkspace()} onOpenWorkspace={(readOnly, allowMigration) => void openExistingWorkspace(readOnly, allowMigration)} /> : null}
        {view === 'runs' && workspace ? <RunsView descriptor={workspace.descriptor} runs={runs} loading={busy === 'runs'} onOpen={(runId) => void activateRun(runId)} /> : null}
        {view === 'inventory' && activeRun ? <InventoryView activeRun={activeRun} page={inventoryPage} filters={inventoryFilters} loading={busy === 'inventory'} canGoBack={inventoryHistory.length > 0} onFilters={setInventoryFilters} onSearch={() => { setAppliedInventoryFilters({ ...inventoryFilters }); setInventoryAfter(undefined); setInventoryHistory([]); setInventoryVersion((value) => value + 1); }} onNext={() => { if (!inventoryPage?.nextAfterSha256) return; setInventoryHistory((history) => [...history, inventoryAfter]); setInventoryAfter(inventoryPage.nextAfterSha256 ?? undefined); }} onPrevious={() => { setInventoryHistory((history) => { const previous = history.at(-1); setInventoryAfter(previous); return history.slice(0, -1); }); }} onSelect={(row) => void openObject(row)} /> : null}
        {view === 'formats' && activeRun ? <FormatsView summary={formatSummary} page={formatPage} tool={formatTool} progress={progress?.stage === 'format_identification' ? progress : null} loading={busy === 'formats' || busy === 'format_tool'} running={busy === 'format_run'} writable={canReview} search={formatSearch} state={formatState} puid={formatPuid} mismatchOnly={formatMismatchOnly} onSearch={setFormatSearch} onState={setFormatState} onPuid={setFormatPuid} onMismatchOnly={setFormatMismatchOnly} onReload={() => { setAppliedFormatFilters({ search: formatSearch, state: formatState, puid: formatPuid, mismatchOnly: formatMismatchOnly }); setFormatAfter(undefined); setFormatVersion((value) => value + 1); }} onProbe={() => void probeFormats()} onRun={() => void runFormats()} onNext={() => { if (formatPage?.nextAfterSha256) setFormatAfter(formatPage.nextAfterSha256); }} /> : null}
        {view === 'findings' && activeRun ? <FindingsView activeRun={activeRun} page={findingsPage} loading={busy === 'findings'} category={findingCategory} severity={findingSeverity} code={findingCode} reviewStatus={findingReviewStatus} search={findingSearch} onCategory={(category) => { setFindingCategory(category); setAppliedFindingFilters((filters) => ({ ...filters, category })); setFindingAfter(undefined); setFindingsVersion((value) => value + 1); }} onSeverity={setFindingSeverity} onCode={setFindingCode} onReviewStatus={setFindingReviewStatusFilter} onSearch={setFindingSearch} onReload={() => { setAppliedFindingFilters({ category: findingCategory, severity: findingSeverity, code: findingCode, reviewStatus: findingReviewStatus, search: findingSearch }); setFindingAfter(undefined); setFindingsVersion((value) => value + 1); }} onNext={() => { if (findingsPage?.nextAfterId) setFindingAfter(findingsPage.nextAfterId); }} onSelect={(finding) => void openFinding(finding)} onExport={(extension) => void exportRun(extension)} /> : null}
      </main>
      {objectDetail ? <ObjectDrawer detail={objectDetail} onClose={() => setObjectDetail(null)} /> : null}
      {findingDetail ? <FindingReviewDrawer detail={findingDetail} canWrite={canReview} busy={busy === 'review'} onClose={() => setFindingDetail(null)} onSetStatus={(status, note) => void updateReview(() => setFindingReviewStatus(findingDetail.finding.id, status, note.trim() || null))} onClear={(note) => void updateReview(() => clearFindingReviewStatus(findingDetail.finding.id, note.trim() || null))} onAddNote={(note) => void updateReview(() => addFindingReviewNote(findingDetail.finding.id, note))} /> : null}
    </div>
  );
}

export default App;

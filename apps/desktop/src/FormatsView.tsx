import type {
  FormatObjectRow,
  FormatPage,
  FormatState,
  FormatSummary,
  FormatToolIdentity,
  ProgressEvent,
} from './types';

const number = new Intl.NumberFormat('en-US');
const decimal = new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 });

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

function titleCase(value: string): string {
  return value.replaceAll('_', ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function shortHash(value: string): string {
  return value.length > 16 ? `${value.slice(0, 12)}…${value.slice(-4)}` : value;
}

function Metric({ label, value, tone }: { label: string; value: number; tone?: string }) {
  return (
    <article className={`format-metric ${tone ?? ''}`}>
      <span>{label}</span>
      <strong>{number.format(value)}</strong>
    </article>
  );
}

function FormatRow({ item }: { item: FormatObjectRow }) {
  const extensionLabel = item.extensionMismatch
    ? 'Mismatch'
    : item.extensionChecked
      ? 'No mismatch'
      : 'Not checked';
  const extensionClass = item.extensionMismatch
    ? 'mismatch-yes'
    : item.extensionChecked
      ? 'mismatch-no'
      : 'mismatch-unchecked';
  return (
    <div className="format-row">
      <div className="format-file">
        <strong>{item.primaryFilename}</strong>
        <span>{shortHash(item.sha256)} · {formatBytes(item.expectedSizeBytes)}</span>
      </div>
      <span className={`state-badge state-${item.state}`}>{titleCase(item.state)}</span>
      <div className="format-identity">
        <strong>{item.primaryFormatName ?? 'Not identified'}</strong>
        <span>{item.primaryFormatVersion || item.primaryMimeType || item.sourceMimeType}</span>
      </div>
      <code>{item.primaryIdentifier ?? '—'}</code>
      <span className={extensionClass}>{extensionLabel}</span>
    </div>
  );
}

export default function FormatsView({
  summary,
  page,
  tool,
  progress,
  loading,
  running,
  writable,
  search,
  state,
  puid,
  mismatchOnly,
  onSearch,
  onState,
  onPuid,
  onMismatchOnly,
  onReload,
  onProbe,
  onRun,
  onNext,
}: {
  summary: FormatSummary | null;
  page: FormatPage | null;
  tool: FormatToolIdentity | null;
  progress: ProgressEvent | null;
  loading: boolean;
  running: boolean;
  writable: boolean;
  search: string;
  state: FormatState | '';
  puid: string;
  mismatchOnly: boolean;
  onSearch: (value: string) => void;
  onState: (value: FormatState | '') => void;
  onPuid: (value: string) => void;
  onMismatchOnly: (value: boolean) => void;
  onReload: () => void;
  onProbe: () => void;
  onRun: () => void;
  onNext: () => void;
}) {
  const percent = progress?.totalItems
    ? Math.min(100, (progress.completedItems / progress.totalItems) * 100)
    : summary?.totalObjects
      ? Math.min(100, (summary.completedObjects / summary.totalObjects) * 100)
      : 0;
  const effectiveTool = tool ?? summary?.tool ?? null;
  return (
    <>
      <header className="topbar">
        <div>
          <span className="eyebrow">EXACT FORMAT IDENTIFICATION</span>
          <h1>Identify real file formats with reproducible PRONOM evidence.</h1>
        </div>
        <div className="source-pill"><span />No OCR · no container expansion</div>
      </header>

      <section className="format-hero">
        <div>
          <span className="section-index">PINNED SIDECAR</span>
          <h2>Siegfried + PRONOM</h2>
          <p>
            One versioned assertion per unique SHA-256 object. Results preserve every match,
            the primary decision, basis, warning, executable digest, and signature release.
          </p>
        </div>
        <div className="tool-card">
          <div><span>Tool</span><strong>{effectiveTool ? `${effectiveTool.toolName} ${effectiveTool.toolVersion}` : 'Not probed'}</strong></div>
          <div><span>Signature</span><strong>{effectiveTool?.signatureVersion ?? '—'}</strong></div>
          <div><span>Executable digest</span><code>{effectiveTool ? shortHash(effectiveTool.executableSha256) : '—'}</code></div>
          <div className="button-row">
            <button className="secondary-action" type="button" disabled={loading || running} onClick={onProbe}>Probe tool</button>
            <button className="primary-action" type="button" disabled={!writable || loading || running || !effectiveTool} onClick={onRun}>
              {running ? 'Identifying formats…' : summary?.latestRunState === 'failed' ? 'Resume identification' : 'Run exact identification'}
            </button>
          </div>
          {!writable ? <small className="format-write-warning">Open the workspace read-write to run this stage.</small> : null}
        </div>
      </section>

      <section className="format-metrics-grid">
        <Metric label="Eligible" value={summary?.eligibleObjects ?? 0} />
        <Metric label="Identified" value={summary?.identified ?? 0} tone="metric-good" />
        <Metric label="Unknown" value={summary?.unknown ?? 0} tone="metric-warn" />
        <Metric label="Ambiguous" value={summary?.ambiguous ?? 0} tone="metric-warn" />
        <Metric label="Extension mismatch" value={summary?.extensionMismatches ?? 0} tone="metric-warn" />
        <Metric label="Tool errors" value={summary?.toolErrors ?? 0} tone="metric-error" />
        <Metric label="PUIDs" value={summary?.distinctPuids ?? 0} />
        <Metric label="Unavailable / empty" value={(summary?.skippedUnavailable ?? 0) + (summary?.empty ?? 0)} />
      </section>

      {(running || progress || summary?.latestFormatRunId) ? (
        <section className="format-progress-panel">
          <div className="format-progress-heading">
            <div><span className="section-index">DURABLE PROGRESS</span><h3>{summary?.latestRunState ? titleCase(summary.latestRunState) : 'Ready'}</h3></div>
            <strong>{decimal.format(percent)}%</strong>
          </div>
          <div className="progress-track"><span style={{ width: `${percent}%` }} /></div>
          <dl>
            <div><dt>Objects</dt><dd>{number.format(progress?.completedItems ?? summary?.completedObjects ?? 0)} / {number.format(progress?.totalItems ?? summary?.totalObjects ?? 0)}</dd></div>
            <div><dt>Bytes considered</dt><dd>{formatBytes(progress?.completedBytes ?? summary?.completedBytes ?? 0)} / {formatBytes(progress?.totalBytes ?? summary?.totalBytes ?? 0)}</dd></div>
            <div><dt>Workers</dt><dd>{progress?.activeWorkers ?? '—'}</dd></div>
            <div><dt>Checkpoint</dt><dd>{progress?.checkpointSequence ?? '—'}</dd></div>
          </dl>
        </section>
      ) : null}

      <section className="format-browser">
        <div className="format-filterbar">
          <input value={search} onChange={(event) => onSearch(event.target.value)} placeholder="Search filename, SHA-256, PUID or format" />
          <select value={state} onChange={(event) => onState(event.target.value as FormatState | '')}>
            <option value="">All states</option>
            <option value="identified">Identified</option>
            <option value="unknown">Unknown</option>
            <option value="ambiguous">Ambiguous</option>
            <option value="empty">Empty</option>
            <option value="skipped_unavailable">Unavailable</option>
            <option value="tool_error">Tool error</option>
            <option value="uninspected">Uninspected</option>
          </select>
          <input className="puid-filter" value={puid} onChange={(event) => onPuid(event.target.value)} placeholder="PUID, e.g. fmt/61" />
          <label className="checkbox-filter"><input type="checkbox" checked={mismatchOnly} onChange={(event) => onMismatchOnly(event.target.checked)} />Mismatch only</label>
          <button className="secondary-action" type="button" disabled={loading} onClick={onReload}>{loading ? 'Loading…' : 'Apply'}</button>
        </div>
        <div className="format-table-head"><span>Content object</span><span>State</span><span>Exact format</span><span>PUID</span><span>Extension</span></div>
        <div className="format-table-body">
          {page?.items.map((item) => <FormatRow item={item} key={item.contentObjectId} />)}
          {!loading && page?.items.length === 0 ? <div className="table-empty">No objects match the current format filters.</div> : null}
        </div>
        <footer className="table-footer">
          <span>{number.format(page?.totalFiltered ?? 0)} objects</span>
          <button className="secondary-action" type="button" disabled={!page?.hasMore || loading} onClick={onNext}>Next page</button>
        </footer>
      </section>
    </>
  );
}

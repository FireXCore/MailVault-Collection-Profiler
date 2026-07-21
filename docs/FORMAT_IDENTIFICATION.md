# Exact format identification

## Purpose

The physical inventory answers **which bytes exist and where they occurred**. Exact format
identification answers **what technical format those bytes represent**. It remains part of the
profiler because it operates on immutable content identity and technical evidence, not document
meaning.

Alpha 4 uses Siegfried with PRONOM signatures. A PUID is stored as an assertion produced by a named,
hashed executable and signature database. It is not promoted to canonical MailVault metadata.

## Product boundary

Included:

- exact format and version;
- PUID and MIME assertion;
- all candidate matches and deterministic primary match;
- ambiguity, unknown and tool-error states;
- extension/signature evidence;
- tool/signature provenance;
- progress, checkpoint, resume and filtering.

Excluded:

- archive extraction;
- OCR or text extraction;
- content preview;
- malware execution or sandboxing;
- semantic or procurement classification;
- RMS writes.

## Why Siegfried + PRONOM

Siegfried supports machine-readable JSON, list-file input, multiple identifiers and configurable
parallel scanning. PRONOM provides persistent format identifiers and signature data maintained by
The National Archives. Alpha 4 pins:

```text
Siegfried: 1.11.6
PRONOM signature: v124
```

The version pair is an evidence contract, not a floating dependency. Updating either requires a new
configuration fingerprint and new assertions; historical assertions remain preserved.

## Supply-chain contract

`scripts/install-siegfried.ps1`:

1. retrieves the named upstream GitHub release through the REST API;
2. selects exactly one Windows x64 ZIP and rejects Win7 assets;
3. requires a GitHub-provided `sha256:` asset digest;
4. verifies asset size and SHA-256 before extraction;
5. installs `sf.exe` and obtains `default.sig`;
6. probes the runtime and requires Siegfried `1.11.6` and PRONOM `v124`;
7. writes `tool-manifest.json` with provenance and local SHA-256 values.

`scripts/verify-siegfried.ps1` repeats the local hash and runtime checks. Release installers bundle
these verified files using Tauri resources. Generated binaries are ignored by Git.

## Windows JSON compatibility

Siegfried `1.11.6` escapes scanned filenames in JSON output, but its JSON header writes the
configured signature value directly. Supplying an absolute Windows signature path therefore emits
unescaped backslashes and invalid JSON. The profiler does not mutate or heuristically repair tool
output. Instead, every probe and identification batch uses:

```text
-home <signature-directory>
-sig  <signature-filename>
```

The sidecar still loads the exact canonical, SHA-256-verified signature file, while the JSON header
contains the portable value `default.sig`. Installer, verifier and Rust runner share this contract,
and Windows CI executes a regression test for it.

## Data model

Migration `0006_exact_format_identification.sql` adds:

- `format_runs`: stage-level identity, configuration and progress;
- `format_observations`: one normalized current projection per content object/run;
- `format_matches`: every returned match, including non-primary alternatives;
- `format_checkpoints`: durable last committed SHA-256 and sequence;
- format projection columns and indexes on `content_objects`.

A format run is attached to a completed physical-profile baseline. The current projection can be
rebuilt from versioned observations.

## States

| State | Meaning |
|---|---|
| `uninspected` | no completed observation exists |
| `identified` | one decisive identifier remains |
| `unknown` | no viable identifier was returned |
| `ambiguous` | more than one decisive identifier remains |
| `empty` | the content object is zero bytes; sidecar is not invoked |
| `skipped_unavailable` | physical object is unavailable; sidecar is not invoked |
| `tool_error` | path resolution, process, parse or per-file tool failure |

Extension-only candidates do not create false ambiguity when stronger byte, container, XML or text
signature evidence exists.

## Primary assertion selection

All matches are retained. The primary assertion is selected deterministically for display:

1. viable identifiers only;
2. container evidence;
3. byte evidence;
4. XML evidence;
5. text evidence;
6. other evidence;
7. extension-only evidence;
8. PRONOM and warning-free results receive deterministic tie-break preference.

This is a presentation projection. It does not delete alternatives or claim certainty when the
run remains ambiguous.

## Extension evidence

MailVault stores content-addressed objects without reliable filename extensions. The stage may
create an ephemeral symbolic alias named with the preferred normalized extension and invokes
Siegfried with symlink following enabled. The alias contains no file copy and is removed with the
batch workspace.

`extension_checked=true` is stored only when that alias was actually created. If the platform or
permissions prevent alias creation, identification falls back to the canonical object path and the
UI displays **Not checked**, not a false “No mismatch.”

## Execution model

```text
read eligible content objects in SHA-256 order
  → skip unavailable and zero-byte objects
  → resolve canonical path beneath archive root
  → create bounded batch workspace
  → invoke one sidecar process for the batch
  → stream and bound stdout/stderr
  → parse JSON and reconcile every input path
  → adaptively split a failed batch until the failing object is isolated
  → commit observations + matches + checkpoint in one transaction
  → emit structured progress
```

Defaults:

```text
batch size: 2048
workers: auto, conservatively capped
process timeout: 900 seconds
resume: enabled
container expansion: disabled
```

Batch size and worker defaults remain benchmark inputs; they are not performance claims for every
storage device.

## Safety controls

- dedicated exclusive workspace lock;
- archive-root containment check for every source path;
- no shell command interpolation;
- explicit executable/signature paths;
- process timeout with kill/wait cleanup;
- 64 MiB stdout and 4 MiB stderr bounds;
- path lines containing CR/LF are rejected;
- no `-z` archive expansion;
- no attachment execution or renderer;
- failed/incomplete run is not published as complete.

## Resume and versioning

A configuration fingerprint includes:

- tool name/version and executable SHA-256;
- signature version and SHA-256;
- extension-evidence model;
- batch size, worker count and timeout;
- container-expansion setting.

A crash-left `running` run may resume only when the fingerprint matches and the exclusive workspace
lock can be acquired. A tool/signature/configuration change starts a separate run.

## UI

The **Exact formats** view provides:

- tool probe and version display;
- aggregate state counts and byte/object progress;
- PUID count and extension mismatch count;
- filters for state, PUID, text and mismatch;
- cursor pagination;
- object rows showing primary assertion and whether extension evidence was checked.

Screenshots are sanitized release illustrations, not fabricated runtime claims.

## References

- Siegfried repository and release notes: https://github.com/richardlehane/siegfried
- PRONOM registry: https://www.nationalarchives.gov.uk/PRONOM/
- DROID and format identification guidance: https://www.nationalarchives.gov.uk/information-management/manage-information/preserving-digital-records/droid/
- Tauri resource bundling: https://v2.tauri.app/develop/resources/
- GitHub release-asset REST representation: https://docs.github.com/en/rest/releases/assets

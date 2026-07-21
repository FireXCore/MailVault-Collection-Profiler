# Security model

## Trust boundaries

Untrusted inputs include MailVault database metadata, canonical locators, filenames, sidecar output
and local configuration. The canonical archive is read-only; the profiler workspace is writable.

## Invariants

- source SQLite is opened read-only and snapshotted consistently;
- workspace must be outside the source archive;
- every object path is canonicalized and constrained beneath the source root;
- no attachment is executed or rendered;
- no command shell is built from archive data;
- exact-format sidecar and signature are versioned and SHA-256 recorded;
- process output and runtime are bounded;
- archive/container expansion is disabled;
- one exact-format writer may hold a workspace lock;
- incomplete runs cannot be marked complete;
- telemetry is disabled by default.

## Sidecar supply chain

The Windows acquisition script requires a GitHub release-asset SHA-256 digest, verifies size/hash,
probes the observed tool/signature versions and generates a local manifest. The release workflow
bundles the verified resources. Runtime probing fails closed when the expected version contract is
not met.

## Extension aliases

Ephemeral symbolic aliases may expose a preferred extension to the identifier without copying the
content object. Alias failure falls back to canonical path identification and records extension as
not checked. Alias paths are generated from SHA-256 and a normalized extension.

## Data exposure

Profiler SQLite, raw logs and tool errors may contain paths, filenames, domains and subjects. They
must not be attached to public issues or GitHub releases.

## Out of scope

Alpha 4 is not an antivirus sandbox, document renderer, archive extractor or malware detonation
environment. A successful format identification does not mean content is safe to open.

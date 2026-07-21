# Security policy

## Supported versions

During `0.x`, security fixes are provided for the latest tagged prerelease and the current default
branch. Older alpha artifacts are not maintained after a corrected release is published.

## Report a vulnerability privately

Use GitHub **Private vulnerability reporting** for vulnerabilities involving:

- source archive mutation;
- path traversal or canonical-locator escape;
- SQLite snapshot or integrity handling;
- attachment execution or active-content rendering;
- command injection;
- unsafe sidecar execution;
- sensitive logging or telemetry;
- installer or update integrity;
- release workflow or secret exposure.

Do not open a public issue until maintainers have coordinated disclosure.

## Never submit real evidence

Use a minimal synthetic fixture. Never attach:

- MailVault databases;
- profiler databases;
- EML files;
- attachments;
- credentials or certificates;
- full private paths;
- message subjects or addresses;
- unredacted runtime evidence.

## Security invariants

- canonical MailVault source is read-only;
- workspace and evidence roots must not overlap the source archive;
- unsupported schemas fail closed;
- path-containment failure prevents file access;
- attachments are not executed, rendered or extracted;
- source-derived strings are untrusted metadata;
- arbitrary shell command strings are not built from archive data;
- explorer reads the active profiler database through read-only/query-only connections;
- telemetry is absent by default;
- release artifacts are checksummed; code signing remains an explicit separate requirement.

## Disclosure response

A private report should include affected version, platform, impact, reproduction with synthetic data
and any proposed mitigation. Maintainers will acknowledge, validate, prepare a fix, define a release
and coordinate public disclosure according to severity.

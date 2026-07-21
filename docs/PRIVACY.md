# Privacy and data handling

MailVault Collection Profiler is designed for local processing.

## Network behavior

The profiling engine and desktop runtime do not require a cloud API, account or telemetry service.
Network access may still occur during installation or development when downloading npm crates,
Rust crates, Tauri tooling, WebView2, WiX or release artifacts.

## Data that remains local

- canonical MailVault archive;
- consistent source database snapshot;
- profiler database and checkpoints;
- filenames and filename variants;
- message subject and sender-domain metadata;
- SHA-256 identities;
- technical findings;
- progress and runtime evidence.

## Derived data is still sensitive

A profiler database contains no need for attachment payload execution, but it can disclose business
relationships and collection structure. Protect it with the same access discipline used for the
source archive.

## Public issue policy

Do not attach:

- archive databases;
- profiler databases;
- EML files;
- attachments;
- private paths;
- message subjects;
- sender or recipient addresses;
- credentials;
- unredacted logs.

Use synthetic fixtures and aggregate counts. Documentation screenshots in this repository use demo
paths and synthetic metadata.

## Review data and sanitized exports

Finding decisions and notes are local profiler metadata. They are not uploaded and are not stored in MailVault. Sanitized export omits absolute paths, filenames, addresses, subjects, full locators and raw notes; short SHA-256-derived tokens provide correlation without exposing source identifiers.

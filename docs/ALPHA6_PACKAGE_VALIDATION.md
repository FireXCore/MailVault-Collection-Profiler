# Alpha 6 package validation

The Alpha 6 source package was assembled from the canonical Alpha 4 source archive and includes
workspace schema 7. Before delivery, the following package-level checks passed:

- application, npm and Tauri versions align at `0.1.0-alpha.6`;
- MSI maps to `0.1.0.6`;
- tag/version gate accepts `v0.1.0-alpha.6`;
- documentation/privacy and local-link gates pass;
- JSON and TOML parse successfully;
- SQLite migrations 1 through 7 apply from an empty database;
- migration 6 to 7 preserves an existing succeeded run;
- `PRAGMA quick_check` is `ok` and `foreign_key_check` is empty;
- `git diff --check` reports no whitespace errors.

Rust compilation, strict Clippy, the full test suite, Tauri installers and the real MailVault acceptance
matrix must run on the owner's Windows environment. This document deliberately does not claim those
runtime gates before their evidence exists.

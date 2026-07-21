# Privacy

Processing is local. The application does not require cloud upload or telemetry.

Sensitive derived data can include:

- archive and workspace paths;
- filenames and filename variants;
- sender domains and message subjects;
- SHA-256 content identities;
- PUID/format assertions tied to private objects;
- tool errors and review notes.

Exact-format identification invokes a local bundled sidecar only. The sidecar receives local file
paths and writes machine-readable output to the profiler process. No attachment bytes are sent to
GitHub, PRONOM or another service during runtime identification.

The build-time sidecar installation script accesses upstream release services to obtain the pinned
tool/signature resources. This happens during development/CI, not while profiling an archive.

Publish only sanitized aggregate exports. Do not publish profiler databases, source snapshots, raw
progress logs or content-level format lists from a private collection.

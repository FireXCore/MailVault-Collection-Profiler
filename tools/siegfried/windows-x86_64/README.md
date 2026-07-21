# Generated Siegfried release resources

This directory is populated by `scripts/install-siegfried.ps1` during Windows CI and release builds.
The generated files are intentionally not committed:

- `sf.exe`
- `default.sig`
- `tool-manifest.json`

The installer script downloads the pinned v1.11.6 Windows x64 release asset through the GitHub REST
API, verifies the asset digest and size, acquires the PRONOM v124 signature, probes the resulting
runtime and records the executable/signature SHA-256 values in `tool-manifest.json`.

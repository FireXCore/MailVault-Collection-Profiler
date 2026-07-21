# Third-party notices

MailVault Collection Profiler can bundle the following independently maintained component in its
Windows installers.

## Siegfried

- Project: `richardlehane/siegfried`
- Pinned release: `v1.11.6`
- License: Apache License 2.0
- Copyright: Richard Lehane, Ross Spencer and contributors
- Purpose: signature-based exact file-format identification

The generated Windows release also includes a pinned `default.sig` signature database built from
The National Archives' PRONOM registry, version `v124`. The build records SHA-256 digests for both
the executable and signature file in `tool-manifest.json`.

Siegfried and PRONOM are not affiliated with FireXCore. Their names identify the upstream tools and
registries used to produce technical format assertions.

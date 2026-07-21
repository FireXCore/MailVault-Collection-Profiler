# Real archive baseline

The initial architecture and acceptance gates are grounded in the supplied private MailVault
collection. These values are not sample data and are not hard-coded as general compatibility
requirements.

## Historical supplied baseline

| Metric | Observed |
|---|---:|
| Archive scale | approximately 20–30 GB |
| Messages | 17,296 |
| Message occurrences | 17,307 |
| MIME parts | 54,450 |
| Attachment occurrences in the broader supplied metadata inventory | 21,946 |
| Messages with attachment metadata | 10,020 |
| Unique attachment SHA-256 values | 13,592 |
| Blob rows | 13,684 |
| Message relationships | 12,115 |
| Participant rows | 51,101 |
| Known security-excluded message | 1 |

Derived from the supplied attachment inventory:

| Metric | Observed |
|---|---:|
| Attachment occurrence bytes | 9,954,841,724 bytes |
| Unique attachment payload bytes | 6,442,427,318 bytes |
| Repeated attachment occurrences | 8,354 |
| Exact duplicate occurrence ratio | 38.07% |
| Unique binaries with multiple normalized filenames | 367 |
| Normalized filenames referring to multiple hashes | 1,123 |
| Unique zero-byte binary | 1 |
| Zero-byte occurrences | 16 |

## Validated alpha.3 adapter run

The controlled `0.1.0-alpha.3` run recorded the following source-contract and inventory aggregates:

| Metric | Recorded |
|---|---:|
| Accounts | 1 |
| Messages | 17,296 |
| Message occurrences | 17,307 |
| MIME parts | 54,450 |
| Attachment occurrences under the current adapter contract | 18,552 |
| Blob rows / content objects | 13,684 |
| Blob bytes | 6,467,253,277 |
| Content occurrences | 22,068 |
| Message relationships | 12,115 |
| Participant rows | 51,101 |
| Same hash with different names | 375 |
| Same normalized name with different hashes | 1,107 |
| Zero-byte content objects | 1 |
| Missing physical objects | 1 |

## Reconciliation boundary

The historical value `21,946` and the alpha.3 adapter value `18,552` are retained as separate
metrics. The project does not silently replace one with the other. The difference must remain
traceable to the role, filtering and occurrence boundaries used by the source inventory and the
current physical-attachment contract.

A future compatibility or benchmark report must:

1. name the exact occurrence definition used;
2. report excluded roles or records explicitly;
3. preserve source and snapshot aggregate comparisons;
4. avoid presenting either count as a universal compatibility constant.

See [Validation evidence — 0.1.0-alpha.3](VALIDATION_0.1.0-alpha.3.md).

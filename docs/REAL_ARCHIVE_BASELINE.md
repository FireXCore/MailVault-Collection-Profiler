# Real archive baseline

The project is designed and validated against a private MailVault collection with approximately
20–30 GB of evidence.

| Metric | Recorded |
|---|---:|
| Messages | 17,296 |
| Message occurrences | 17,307 |
| MIME parts | 54,450 |
| Explicit attachment-role occurrences | 18,552 |
| Historical broader attachment metadata baseline | 21,946 |
| Content objects / blob rows | 13,684 |
| Content occurrences | 22,068 |
| Message relationships | 12,115 |
| Participants | 51,101 |
| Blob bytes | 6,467,253,277 |
| Available objects | 13,683 |
| Known missing security-excluded object | 1 |
| Zero-byte content object | 1 |
| Physical findings | 1,484 |

The three attachment-related counts represent different scopes and must not be collapsed:

- 18,552: MIME parts with explicit `role=attachment` in the current adapter contract;
- 21,946: historical attachment metadata inventory scope;
- 22,068: all content-bearing occurrences linked to content objects.

Alpha 3 completed the physical profile in roughly 400 seconds on the user's Windows system. Alpha 4
must record its own exact-format duration and memory evidence before format performance is claimed.

Current source MIME distribution includes approximately 2,890 generic OLE objects and 137
`application/octet-stream` objects, which is the principal reason exact PRONOM identification is
required.

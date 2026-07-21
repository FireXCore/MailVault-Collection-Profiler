# GUI guide

## Start and collection setup

Select the canonical MailVault root, run preflight and create or open a profiler workspace. The
source is always read-only.

## Runs

Choose a completed physical-profile run. Alpha 4 attaches exact format assertions to this baseline;
it does not create a new archive.

## Physical inventory

Search content objects and inspect filename/message occurrence history. SHA-256 is identity;
filename is evidence.

## Findings

Review physical findings using append-only statuses. Review state never modifies MailVault.

## Exact formats

The **Exact formats** view:

1. resolves the bundled or configured Siegfried sidecar;
2. displays observed tool/signature identity;
3. shows eligible object and byte totals;
4. starts or resumes the versioned format run;
5. streams exact object progress;
6. exposes state, PUID, search and mismatch filters.

Extension status meanings:

- **Mismatch** — a safe extension alias was checked and the tool reported mismatch evidence;
- **No mismatch** — an alias was checked and no mismatch warning was reported;
- **Not checked** — no reliable extension alias was evaluated.

Unknown and ambiguous are evidence queues, not application failures. Tool errors require technical
review.

## Screenshots

![Exact format dashboard](assets/screenshots/07-exact-format-identification-ready.png)

![Format assertion detail](assets/screenshots/08-format-assertion-detail.png)

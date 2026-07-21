# Screenshot and documentation media policy

Repository screenshots must not reveal private archive or workstation information.

## Required sanitization

Remove or replace:

- drive labels and private volume names;
- user profile paths;
- company, client or supplier names;
- email addresses and domains;
- message subjects;
- real filenames;
- full content hashes from private material;
- taskbar, notifications and unrelated applications;
- timestamps that identify a private incident when unnecessary.

## Repository images

- `01-collection-setup-preflight.png` and `02-profile-running.png` are sanitized application
  captures with demo paths.
- Explorer, findings, content-detail and CLI images use synthetic documentation data modeled on the
  implemented interface and command contracts.
- `alpha3/01-start.png`, `alpha3/03-runs.png` and `alpha3/06-findings.png` are public release-tour
  images with sanitized or synthetic paths and metadata.
- Baseline counts may reflect documented aggregate evidence; they are not compatibility constants.

## Path convention

Documentation uses:

```text
D:\MailVault-Demo
D:\MailVault-Profiler-Workspace
D:\MailVault-Profiler-Evidence
```

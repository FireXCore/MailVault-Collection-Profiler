## Summary

Describe the user-visible or architectural change and why it is necessary.

## Scope

- Components changed:
- Explicitly unchanged:
- Related issue:

## Evidence-processing invariants

Explain how this change preserves or intentionally evolves each applicable invariant:

- canonical archive remains read-only;
- workspace remains outside the archive;
- snapshot publication remains atomic;
- canonical locators remain containment-checked;
- attachment payloads are never executed or uploaded;
- findings and checkpoints remain collection/run scoped.

## Failure and recovery behavior

Describe crash points, retries, partial files, checkpoints, idempotency, and cleanup behavior.

## Compatibility and migrations

Describe schema/layout compatibility, profiler migrations, release-version impact, and rollback behavior.

## Performance and progress

State the measured units, benchmark fixture, throughput/memory impact, worker behavior, and whether progress remains monotonic.

## Security and privacy

State new trust boundaries, filesystem access, command execution, network access, and derived metadata exposure.

## Validation evidence

Paste concise commands and results. Use synthetic fixtures and sanitized paths only.

```text
cargo test ...
npm run ...
```

## Documentation and release impact

List updated documentation, screenshots, changelog entries, and release notes.

## Checklist

- [ ] `scripts/quality.ps1` or `scripts/quality.sh` passes.
- [ ] Source archive mutation is impossible or explicitly rejected.
- [ ] Tests use synthetic data only.
- [ ] No archive, EML, attachment, profiler DB, evidence bundle, token, certificate, private path, or unredacted screenshot is committed.
- [ ] New errors have stable context and safe messages.
- [ ] Progress uses measurable backend units.
- [ ] Rerun and failure behavior is covered.
- [ ] Documentation and release notes are updated when behavior changes.
- [ ] Breaking compatibility changes are called out explicitly.

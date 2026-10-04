# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

`AttachmentProgressSink` gains `begin_phase(step)` and `end_phase(step)`, which send a `StartPhase` event
through a streaming sink and are dropped by a discarding one. A step that fails reports no end. Docs:
[host-documents-and-attachments.md](../host-documents-and-attachments.md#materialisation-progress).

**Code issues.** None recorded for this package; none affected.

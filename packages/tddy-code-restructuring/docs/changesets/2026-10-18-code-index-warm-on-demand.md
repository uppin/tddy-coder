# 2026-10-18 — ServerChatter lists every active phase
**Type:** Feature

`ServerChatter::active_phases` returns one `ActivePhase { title, message, percentage }` per work-done token
between `begin` and `end`, in begin order (re-exported from `backends::rust`); `lost_sight` drops a phase
whose `end` was lost in a notification gap at the next quiescence; the `"begin"` kind is one `BEGIN`
constant. Unit tests in `chatter.rs`. `oversized-file-backends-rust` re-measured, unchanged (3,025).

Docs: [readiness-and-gates.md](../readiness-and-gates.md#readiness). Cross-package entry:
[2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).

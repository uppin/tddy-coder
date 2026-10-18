# 2026-10-18 — fake_lsp holds a crate-graph load until signalled
**Type:** Testing

`fake_lsp --loads-crate-graph-on-signal <path>` begins two overlapping phases (`Fetching`, `Indexing`) under a
non-quiescent status and holds until `<path>` exists, answering requests meanwhile, so index-daemon tests
of "joined mid-load" and "held until ready" need no sleeps.

Docs: [workspace-root.md](../workspace-root.md). Cross-package entry:
[2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).

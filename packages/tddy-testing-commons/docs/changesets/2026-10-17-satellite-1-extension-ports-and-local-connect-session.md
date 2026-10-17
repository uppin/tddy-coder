# 2026-10-17 — WorkspaceGraph

**Type:** Feature

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

`dependency_graph::WorkspaceGraph` reads `cargo metadata --no-deps` and answers `workspace_crates`,
`names_directly` (panics on a non-member) and `reaches`. It counts `[dependencies]` and
`[build-dependencies]` including optional, table-form and workspace-inherited entries, and not
`[dev-dependencies]`. The two hand-rolled manifest walkers in the `tddy-daemon-livekit` and
`tddy-daemon-auth` boundary suites are gone.

Docs: [dependency-graph.md](../dependency-graph.md).

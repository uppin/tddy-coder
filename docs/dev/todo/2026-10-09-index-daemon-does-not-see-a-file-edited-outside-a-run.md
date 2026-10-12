# 2026-10-09 — the warm index daemon does not see a file edited outside a run

**Category:** Bug (tddy-index-daemon)
**Source:** #reshape 4/19 (`extract-method-clean`), moved out of item V of
`2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md`, which #reshape 4 resolves and
deletes

On #524, after hand edits added `find_registered_project` to `service_util.rs`, plan `14` was refused with `project: _`
and `projects_dir: Path` (unsized, by value) in the extracted signatures. After `./run-index-daemon --stop` and a cold start,
the same plan got `project: ProjectData` and passed. The warm server had answered from the tree as it was at its start:
it is told about the documents a run opens, never about files changed on disk between runs. Plans `11`–`13` had passed
against the same stale server and were re-checked on a fresh one before applying.

Until this is fixed: **restart the index daemon after any hand edit**, or its checks are about a tree that no longer
exists.

## Why deferred

It is the daemon's (`tddy-index-daemon`), not the restructure engine's: the fix is a file watcher forwarding
`workspace/didChangeWatchedFiles` to the server it keeps warm, or a tree fingerprint checked per request.
`2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md` covers the readiness latch's watcher, not on-disk edits.

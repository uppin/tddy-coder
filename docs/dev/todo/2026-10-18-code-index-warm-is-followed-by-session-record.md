# 2026-10-18 — The code-index warm is followed by session record, so a restarted host forgets it

**Category:** Future enhancement
**Source:** `code-pane-index-status-strip` changeset ([#652](https://github.com/uppin/tddy-coder/pull/652))

## What is left

`WatchCodeIndex(session)` answers from `SessionIndexProgress`, an in-memory record the daemon fills
when it warms a session at start. After the host restarts (routinely, for Tddy Desktop), every
existing session's record is empty: the watch ends with no message, the header shows nothing and the
Code pane's strip reads *Code index not warmed for this session*. Nothing warms the worktree until the
first definition or hover, which then answers empty while rust-analyzer loads — the "No definition
found" a user reads as a broken index.

## What would close it

Follow the warm by **worktree** rather than by session record: a pane-side watch that forwards to
`code_index.Warm` for its worktree. `Warm` is idempotent — a root whose graph is loaded answers
`ready` at once — so following it also *starts* the load for a forgotten session, and covers the
starts that never warm (see
[2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md](2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md)).
Cost: one `code_navigation` RPC with the worktree service's authorisation, a forwarding handler in
`tddy-daemon-rpc`, and switching `useCodeIndexProgress`'s source.

## Why it was left

The strip shipped on the existing session stream to show progress for new sessions first; the worktree
watch is a proto and daemon change of its own.

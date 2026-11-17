# 2026-11-17 — Orchestrators watch the sessions they can see

**Type:** Feature · `#activity-wake` 2/3, PR [#930](https://github.com/uppin/tddy-coder/pull/930)

`DaemonProjectHome` implements the watch port, and every successful, object-shaped answer of
`ExecToolRpcHandler::execute_tool` (now delegating to `answer_execute_tool`) passes through
`with_watch_events`, which inserts the caller's waiting `watchEvents` — so an error answer
consumes nothing. Full record:
[`docs/dev/changesets/2026-11-17-session-watches.md`](../../../../docs/dev/changesets/2026-11-17-session-watches.md).

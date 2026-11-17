# 2026-11-17 — Orchestrators watch the sessions they can see

**Type:** Feature · `#activity-wake` 2/3, PR [#930](https://github.com/uppin/tddy-coder/pull/930)

`project_home_watch_tools` defines `CreateSessionWatch` / `ListSessionWatches` /
`RenewSessionWatch` / `CancelSessionWatch` and their `ProjectHomeWatchPort`, with every refusal
worded to say what to do; the project-home catalogue offers them to the project-home role only.
Full record:
[`docs/dev/changesets/2026-11-17-session-watches.md`](../../../../docs/dev/changesets/2026-11-17-session-watches.md).

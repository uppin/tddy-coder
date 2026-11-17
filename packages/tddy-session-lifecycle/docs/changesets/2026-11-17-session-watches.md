# 2026-11-17 — Orchestrators watch the sessions they can see

**Type:** Feature · `#activity-wake` 2/3, PR [#930](https://github.com/uppin/tddy-coder/pull/930)

`DaemonSessionHost::with_session_watches` / `session_watches` install the daemon's watch store on
the host (`svc_session_watches_wiring.rs`), for the exec-tool handler and `DaemonProjectHome` to
reach. Full record:
[`docs/dev/changesets/2026-11-17-session-watches.md`](../../../../docs/dev/changesets/2026-11-17-session-watches.md).

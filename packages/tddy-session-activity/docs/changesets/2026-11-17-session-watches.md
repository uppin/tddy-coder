# 2026-11-17 — Orchestrators watch the sessions they can see

**Type:** Feature · `#activity-wake` 2/3, PR [#930](https://github.com/uppin/tddy-coder/pull/930)

`session_watches` is an orchestrator's watches over the sessions it can see: `SessionWatchStore`
keeps a filter (sessions by id and/or started-by-me, six `WatchKind`s) with its own position in
the project notification log and a TTL ≤ 24 h, persisted per project and delivered exactly once
by `take_delivery`. An activity-status notification now carries the status it was raised for
(`activity_status`), kept in the log's JSON, so `turn_finished` and `waiting_for_input` are told
apart. Full record:
[`docs/dev/changesets/2026-11-17-session-watches.md`](../../../../docs/dev/changesets/2026-11-17-session-watches.md).

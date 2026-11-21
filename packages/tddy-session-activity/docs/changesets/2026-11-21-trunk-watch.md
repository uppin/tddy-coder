# 2026-11-21 — New trunk commits reach every live orchestrator as activities

**Type:** Feature

`trunk_watch` (new): `TrunkWatcher`, one poller per project shared by its subscribed orchestrators and dropped with the last live one; `WatchKind::TrunkCommit` and the owner's own trunk records matching its watch; the project log keeps `TrunkCommit` records though they are activity, and neither the inbox reads nor the drawer's stream carry them. See [session-notifications.md § The trunk watch](../session-notifications.md#the-trunk-watch-trunk_watch). Full record: [`docs/dev/changesets/2026-11-21-trunk-watch.md`](../../../../docs/dev/changesets/2026-11-21-trunk-watch.md) (#983, `#orchestra` 5/9).

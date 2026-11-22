# 2026-11-21 — New trunk commits reach every live orchestrator as activities

**Type:** Feature

`svc_trunk_watch_wiring` (new): `DaemonSessionHost::with_trunk_watch`, `trunk_watcher`, `spawn_trunk_watch` and the `orchestrator_running` port; `HostTrunkSource` reads a Local gh project through the host `gh` and any other through its vault account, the token re-read on every poll. Full record: [`docs/dev/changesets/2026-11-21-trunk-watch.md`](../../../../docs/dev/changesets/2026-11-21-trunk-watch.md) (#983, `#orchestra` 5/9).

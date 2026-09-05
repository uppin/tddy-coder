# 2026-07-12 — `usage_watcher` moved to `tddy-core`

**Type:** Refactor

the `SessionUsageEmitter` / `spawn_usage_watcher` module moved out of `tddy-daemon` (to `tddy-core`, so `tddy-coder`'s `run_daemon` can call it); the `usage_watcher` integration test now imports from `tddy_core`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#295](https://github.com/uppin/tddy-coder/pull/295). (tddy-daemon)

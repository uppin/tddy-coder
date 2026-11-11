# 2026-11-10 — tddy-presenter: the usage watcher removed

`usage_watcher` (`SessionUsageEmitter`, `spawn_usage_watcher`, `spawn_session_usage_watcher`) and
`PresenterEvent::TokenUsageUpdated` deleted: the daemon streams every session's usage
(`ActivityService.StreamSessionUsage`). `#usage-binding 2/5`, PR [#891](https://github.com/uppin/tddy-coder/pull/891).

# 2026-10-22 — The session host holds the operator channel; no tddy-telegram

**Type:** Architecture

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md).

- Dropped the `tddy-telegram` dependency and its four re-exports (`active_elicitation`,
  `elicitation`, `telegram_github_link`, `telegram_tracked_session`); their one reader,
  `tddy-daemon`'s Telegram wiring, names `tddy_telegram::…` directly.
- `DaemonSessionHost::new` takes `operator_channel: SharedOperatorChannel` in place of
  `Option<SharedPresenterEventSink>`; tests without a chat surface pass `NoOperatorChannel`.
- `connection_service.rs` re-measured ±0 (`oversized-file-connection-service.md`, still over 500).
  Analyzer: nothing at CRAP ≥ 30 in the changed production files (618 tests, join 76.3%).

[session-service.md § The presenter observer](../session-service.md#the-presenter-observer).

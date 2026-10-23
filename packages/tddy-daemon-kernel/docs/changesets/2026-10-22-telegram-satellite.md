# 2026-10-22 — The operator channel port

**Type:** Architecture

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md).

- Added `operator_channel`: `OperatorChannel` (`id`, `follows_presenter_events`,
  `on_presenter_event`, `wants_notification`, `on_notification`), `SharedOperatorChannel`, the null
  object `NoOperatorChannel`, and `awaits_operator`, the core's one rule for "this presenter mode
  waits on the operator", shared by the drawer indicator and Telegram's elicitation queue.
- Added `session_notification`: `SessionNotification`, `SessionNotificationKind`,
  `SessionNotificationSource`, moved from `tddy-session-activity` (which re-exports them) so the port
  can name them.
- Removed `presenter_observer` (`PresenterEventSink`, `SharedPresenterEventSink`); the operator
  channel replaced it. Absence became `NoOperatorChannel` rather than `None`.
- Tests: `tests/operator_channel_acceptance.rs` (the gate rule over every mode, the null object, the
  trait default, delivery through the shared port); `tests/telegram_extraction_shape.rs` pins
  `the_connection_service_holds_the_operator_channel_port`.
- Analyzer: nothing at CRAP ≥ 30 in the changed files (167 tests, join 70.3%).

[daemon-kernel.md § Ports](../daemon-kernel.md#ports).

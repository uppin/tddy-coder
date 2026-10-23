# 2026-10-22 — The bus and the presenter observer deliver to the operator channel; no tddy-telegram

**Type:** Architecture

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md).

- Dropped the `tddy-telegram` dependency. The drawer-dot gate calls the kernel's `awaits_operator`;
  the `ListSessions` `pending_elicitation` flag is read by the new `pending_elicitation` module.
- `session_notifications` re-exports `SessionNotification`, `SessionNotificationKind` and
  `SessionNotificationSource` from `tddy_daemon_kernel::session_notification`, and gained
  `OperatorChannelSubscriber`, the bus adapter that registers a `SharedOperatorChannel` (named after
  the channel's `id()`).
- `PresenterObserverDeps` and `spawn_presenter_observer_task` take a `SharedOperatorChannel`. The
  observer is spawned when the channel `follows_presenter_events()` or a bus is present.
- `run_presenter_observer_loop`: CRAP **90** (complexity 9, untested) → **6** (complexity 2), over a
  `PresenterEvents` trait, `follow_presenter_events` (CRAP 5) and `deliver_presenter_event` (CRAP 4),
  with 9 unit tests of the run condition and per-event delivery. No record left.
- Tests: `tests/operator_channel_subscriber_acceptance.rs` (4). Crate: 63 tests; analyzer join
  76.9% → 77.0%.

[session-notifications.md](../session-notifications.md), [presenter-observer.md](../presenter-observer.md).

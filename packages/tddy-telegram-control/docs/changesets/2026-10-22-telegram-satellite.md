# 2026-10-22 — TelegramDaemonHooks is the operator channel; workflow launch leaves pickers.rs

**Type:** Architecture, Refactor

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md). Production lines are counted to the first
`#[cfg(test)]`.

- `TelegramDaemonHooks` implements `tddy_daemon_kernel::operator_channel::OperatorChannel` and is
  built with `TelegramDaemonHooks::new(config, sender, watcher)`. It absorbed
  `TelegramNotificationSubscriber` — its interest filter (`AttentionRequired` from `ActivityStatus`
  only), tracked-first routing and per-session dedupe (a private `LastDeliveredPerSession`) — and
  `telegram_notification_subscriber.rs` was deleted. `on_notification` is 17 lines over
  `recipients_of` and `send_to_every_chat`.
- `telegram_notifier.rs` calls `awaits_operator`: **1,246 → 1,245** (`oversized-file-telegram-notifier.md`,
  still over 500); `on_server_message` 214 → 213 lines, CRAP 31, covered
  (`complexity-telegram-notifier-on-server-message.md`).
- `spawn_telegram_workflow` moved to `telegram_session_control/workflow_launch.rs` (133) and was split
  into `workflow_spawn_deps`, `project_for_workflow_spawn`, `merge_chain_child_integration_base`,
  `explicit_integration_base_ref`, `follow_spawned_workflow` and `announce_started_workflow`: CRAP
  **306** (complexity 17, untested) → **< 9**. Its record was added and closed in this change.
  `pickers.rs` **706 → 617** (`oversized-file-telegram-session-control-pickers.md`, narrowed, still
  over 500).
- Tests: `telegram_operator_channel_acceptance` (the port contract) and
  `telegram_workflow_launch_acceptance` (3; also covers `handle_telegram_agent_callback`, CRAP 30 →
  covered); `telegram_notification_subscriber_unit`, `session_notifications_acceptance` and
  `telegram_claude_cli_activity_alert_acceptance` moved onto the port unchanged in behaviour. Crate:
  172 tests; analyzer join 62.7% → 63.3%.

[architecture.md § Telegram's operator channel](../architecture.md#telegrams-operator-channel).

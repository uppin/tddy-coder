# 2026-10-22 — The daemon wires Telegram's operator channel and reports Telegram's status

**Type:** Architecture, Feature

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md).

- Names `tddy-telegram` directly; `telegram_wiring.rs` builds the chat-side coordinators at
  `tddy_telegram::…` instead of lifecycle's removed re-exports. `tddy-daemon` is the one wiring crate
  allowed to name a Telegram crate.
- `mount_host.rs` installs the hooks as the `SharedOperatorChannel` — or `NoOperatorChannel` — on
  `DaemonSessionHost::new` and, through `OperatorChannelSubscriber`, on the daemon's bus.
- `integrations.rs` creates the `telegram` reporter and reports `configured_state`, so an enabled
  block with an empty `bot_token` shows as `Misconfigured` instead of starting nothing silently.
  `telegram_wiring.rs` wraps the sender in `ReportingSender` and builds `TelegramIdentityCheck`;
  `tasks.rs`'s `spawn_telegram` runs `report_bot_identity` before the inbound dispatcher
  (`TelegramInbound::run`).
- `build_telegram` 7 parameters → 5 (`&Registries`, `&Plugins`), 129 → 103 lines (master 111);
  `RuntimeTasks::spawn` 150 → 128 (master 139). Both still over 60, pre-existing. Analyzer: nothing
  at CRAP ≥ 30 in the changed files (join 70.6% → 71.2%; `spawn_telegram` < 7).

[daemon-endpoint.md § Integration status](../daemon-endpoint.md#integration-status-the-livekit-plugin-and-the-telegram-plugin).

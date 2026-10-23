# 2026-10-22 — The Telegram satellite ratchet

**Type:** Architecture

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md). No production change.

- Added `tests/telegram_is_a_satellite.rs` beside `livekit_is_a_satellite.rs`, on the same
  `WorkspaceGraph`: `the_daemon_reaches_the_telegram_plugin` (non-vacuity),
  `only_telegram_plugins_and_wiring_name_a_telegram_crate` (`PLUGINS` `tddy-telegram`,
  `tddy-telegram-control`; `WIRING` `tddy-daemon`), `the_kernel_reaches_no_telegram_crate` and
  `no_session_crate_reaches_a_telegram_crate`. Dev-dependencies are not counted.
- The operator-channel port lives in `tddy-daemon-kernel`, not here: it carries `ServerMessage`, and
  this crate stays free of `tddy-service`. Telegram reports on this crate's integration status board.

[extension-ports.md § Telegram](../extension-ports.md#telegram-the-operator-channel-and-its-ratchet).

# 2026-10-22 — Telegram on the integration status board; the gate rule leaves for the core

**Type:** Architecture, Feature

Telegram as a satellite; cross-package entry: [2026-10-22-telegram-satellite.md](../../../../docs/dev/changesets/2026-10-22-telegram-satellite.md).

- Added `status`: `TELEGRAM_INTEGRATION_ID` (`telegram`), `configured_state` (`Disabled`,
  `Misconfigured { bot_token }`, `Connecting`), `ReportingSender` (every send reports `Connected` or
  `Unreachable`), `BotIdentity` / `TeloxideBotIdentity` / `report_bot_identity` (the `getMe` check)
  and `redact_bot_token` — no error written to the board, or handed back to a caller, carries the
  bot token, as written or percent-encoded. New dependency: `tddy-daemon-extensions`.
- `elicitation` lost `mode_changed_requires_telegram_elicitation` (the rule is the kernel's
  `awaits_operator`) and `pending_elicitation_for_session_dir` (moved to `tddy-session-activity`).
  `mode_gate_tests` pins that the chat line exists exactly for the modes `awaits_operator` names.
- Only `tddy-telegram-control` and `tddy-daemon` depend on this crate.
- Tests: `tests/telegram_integration_status_acceptance.rs`. Crate: 41 tests; analyzer join 84.4%;
  two leads ≥ 30 (`elicitation_signature_for_mode_changed` 42, `verify_and_extract_telegram_user` 72)
  are per-crate blind spots, exercised from `tddy-telegram-control`, no record.

[telegram-notifier.md § Integration status](../telegram-notifier.md#integration-status).

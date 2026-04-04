# Validate Tests Report — Telegram notifier (`tddy-daemon`)

## Executive summary

Automated tests for the Telegram notifier feature and related packages were run on 2026-04-03. **`./dev cargo test -p tddy-daemon` completed with exit status 0** — all tests passed (64 runnable tests across library, integration, and binary targets; `main` has 0 unit tests). **`./dev cargo test -p tddy-tools` also completed with exit status 0** (61 tests) as a workspace spot-check. There were **no failing tests**. Coverage remains **library- and mock-sender–focused**: the daemon entrypoint does not reference Telegram, and there are no tests for `enabled: true` with empty `chat_ids`, real teloxide sends, or end-to-end status observation from a running process.

## Commands run

| Command | Working directory | Exit status |
|---------|-------------------|-------------|
| `./dev cargo test -p tddy-daemon` | Repo root | 0 |
| `./dev cargo test -p tddy-tools` | Repo root | 0 |

## Results

### `tddy-daemon` (primary)

| Target | Tests | Passed | Failed | Ignored |
|--------|-------|--------|--------|---------|
| `unittests src/lib.rs` | 37 | 37 | 0 | 0 |
| `unittests src/main.rs` | 0 | 0 | 0 | 0 |
| `tests/acceptance_daemon.rs` | 8 | 8 | 0 | 0 |
| `tests/delete_session.rs` | 2 | 2 | 0 | 0 |
| `tests/grpc_spawn_contract.rs` | 1 | 1 | 0 | 0 |
| `tests/list_agents_allowlist_acceptance.rs` | 4 | 4 | 0 | 0 |
| `tests/list_sessions_enriched.rs` | 1 | 1 | 0 | 0 |
| `tests/multi_host_acceptance.rs` | 5 | 5 | 0 | 0 |
| `tests/signal_session.rs` | 3 | 3 | 0 | 0 |
| **`tests/telegram_notifier.rs`** | **3** | **3** | **0** | **0** |
| Doc-tests | 0 | 0 | 0 | 0 |
| **Total (runnable)** | **64** | **64** | **0** | **0** |

**Telegram-specific tests**

- **Unit tests** (`telegram_notifier::acceptance_unit_tests`): `two_segment_label_from_uuid_session_id`, `mask_bot_token_redacts_secret`, `is_terminal_session_status_recognizes_completed_and_failed`, `inactive_session_skips_notification_even_on_transition`.
- **Integration tests** (`tests/telegram_notifier.rs`): `telegram_config_disabled_skips_notifier`, `status_transition_triggers_single_telegram_message_mock`, `terminal_session_not_spammed`.

**Failing tests:** none — no failure names or reasons to report.

### `tddy-tools` (spot-check)

| Target | Tests | Passed | Failed |
|--------|-------|--------|--------|
| Library + binary unit tests | 17 | 17 | 0 |
| `tests/cli_integration.rs` | 16 | 16 | 0 |
| `tests/schema_validation_tests.rs` | 23 | 23 | 0 |
| Other integration tests | 5 | 5 | 0 |
| **Total** | **61** | **61** | **0** |

## Coverage gaps

1. **Daemon wiring (`main.rs`)** — `packages/tddy-daemon/src/main.rs` contains no references to `telegram_notifier`, `TelegramSessionWatcher`, or `on_metadata_tick`. Nothing in the test suite exercises startup of a notifier loop or session polling tied to Telegram. Behavior validated today is **library-only** until integration lands.
2. **Empty `chat_ids` when enabled** — Integration tests use a non-empty `chat_ids: [424242]`. The evaluation report noted that with `telegram.enabled: true` and an empty chat list, transitions may update internal state without visible sends; this is **not covered by an explicit test** asserting zero calls or expected logging.
3. **`send_telegram_via_teloxide`** — Real Bot API traffic is intentionally avoided in tests (mock `TelegramSender`). There is **no automated E2E** against Telegram’s API or a stub HTTP server for the teloxide path.
4. **Session metadata from disk / gRPC** — Existing session tests use metadata for other features; **no test** drives `TelegramSessionWatcher` from the same flows the production daemon would use after wiring (e.g. periodic reads from session directories or RPC-driven updates).
5. **`main` binary test harness** — The `tddy-daemon` binary test target reports **0 tests**, so any future logic in `main.rs` related to Telegram would remain **unexercised** unless covered by integration tests or new unit tests in `lib`.

## Recommendations

1. After wiring Telegram into the daemon, add **at least one acceptance or integration test** that starts the relevant components (or a thin test harness) and asserts `on_metadata_tick` is invoked when session status changes in a way that mirrors production.
2. Add a **focused test** for `enabled: true` with `chat_ids: []` documenting expected behavior (no panics, no sends, and optionally visibility via logging if that is added).
3. Keep **real-network E2E** optional (manual or staging-only) unless the project adopts a contract test or mock HTTP server for teloxide; document the decision in the feature changeset.
4. Run **`cargo clippy -p tddy-daemon`** and full workspace **`./test`** before merge if CI does not already cover the full matrix; this report only executed the two `cargo test` commands above.

**Report generated:** validate-tests subagent, 2026-04-03.

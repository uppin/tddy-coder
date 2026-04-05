# Validate-tests report: Telegram session control (`tddy-daemon`)

**Date:** 2026-04-05

## Commands run

| Command | Environment |
|--------|----------------|
| `export TMPDIR=/var/tddy/tmp-tddy-cargo` | Writable temp base (created if missing). |
| `cd /var/tddy/Code/tddy-coder/.worktrees/telegram-session-control && ./dev cargo test -p tddy-daemon 2>&1` | Primary validation. |
| `./dev cargo check --workspace 2>&1` | Broader workspace compile smoke test (dev profile). |
| `./dev cargo test --workspace --no-run` | **Optional** — started but not run to completion in this session (long-running); terminated after extended wait. Use locally if you need test-binary compilation for every crate. |

## Overall result

- **`cargo test -p tddy-daemon`:** **PASS** — exit code **0**.
- **`cargo check --workspace`:** **PASS** — exit code **0** (`Finished` in ~4 minutes).

## Passing summary (`tddy-daemon`)

| Suite | Passed |
|-------|--------|
| Library unit tests (`src/lib.rs`) | 58 |
| Binary unit tests (`src/main.rs`) | 0 |
| `tests/acceptance_daemon.rs` | 8 |
| `tests/delete_session.rs` | 2 |
| `tests/grpc_spawn_contract.rs` | 1 |
| `tests/list_agents_allowlist_acceptance.rs` | 4 |
| `tests/list_sessions_enriched.rs` | 2 |
| `tests/multi_host_acceptance.rs` | 5 |
| `tests/session_workflow_files_rpc.rs` | 3 |
| `tests/signal_session.rs` | 3 |
| `tests/telegram_notifier.rs` | 3 |
| `tests/telegram_session_control_integration.rs` | 5 |
| `tests/worktrees_acceptance.rs` | 2 |
| `tests/worktrees_rpc.rs` | 4 |
| Doc-tests | 0 |
| **Total** | **100** |

**Telegram-focused tests in this run**

- **Unit (`telegram_session_control`):** 4 tests — `parse_start_workflow_extracts_prompt`, `chunk_telegram_text_respects_limit_and_continuation_markers`, `parse_callback_payload_recognizes_recipe_selection`, `map_elicitation_callback_to_presenter_input_matches_web_encoding`.
- **Integration (`telegram_session_control_integration`):** 5 tests — start workflow + keyboard, recipe + demo_options persistence, plan review chunking + transition, elicitation byte mapping, unauthorized chat denial.

## Failing tests

**None.** All 100 `tddy-daemon` tests passed.

*(Note: `packages/tddy-daemon/telegram_session_control_red_test_output.txt` captures an older red-phase / marker-driven failure log; it is not the result of the current successful run.)*

## Coverage gaps

### `telegram_session_control.rs` (unit)

- **Covered well:** `parse_start_workflow_prompt`, `chunk_telegram_text`, `parse_callback_payload` (recipe branch), `map_elicitation_callback_to_presenter_input` (one multi-select case).
- **Gaps / light coverage:**
  - **`parse_callback_payload`:** Returns `None` for non-`recipe:` payloads; no explicit test for elicitation-only or mixed strings beyond the integration layer.
  - **`chunk_telegram_text`:** Edge cases (`max_utf8_bytes == 0`, empty string, very small max vs continuation suffix) are partially implied but not all enumerated in unit tests.
  - **`parse_demo_options_value`:** Exercised indirectly via integration (`demo_options:{run:true}`), not isolated unit tests for JSON vs YAML normalization failures.
  - **`read_changeset_routing_snapshot`:** Used in integration; no dedicated unit test for malformed YAML or missing file errors.
  - **`TelegramSessionControlHarness`:** Handler behavior is mostly integration-tested; `handle_plan_review_phase` discards `approval_callback` (no assertion that callback data drives workflow server-side — aligns with “harness” scope).
  - **`WorkflowTransitionKind::ElicitationSubmitted`:** Not referenced in current tests (only `PlanReviewApproved` appears in the plan-review test).

### `telegram_notifier` — `InMemoryTelegramSender` keyboard paths

- **`send_message_with_inline_keyboard` and `recorded_with_keyboards`:** Exercised through **session control integration tests** and `drain_outbound_messages`, not through **`telegram_notifier`**’s own `acceptance_unit_tests` (those use `InMemoryTelegramSender` for plain `send_message` / watcher behavior).
- **Gap:** No focused unit test in `telegram_notifier.rs` that asserts `recorded_with_keyboards()` row/column structure after `send_message_with_inline_keyboard`, or that `recorded()` strips keyboards as documented.

### Gaps vs product / architecture (inbound bot, RPC, config)

Aligned with the stated evaluation context (**medium risk; full bot/RPC not wired**):

| Area | Status |
|------|--------|
| **Inbound Telegram bot (teloxide update loop)** | `telegram_session_control` is a **library harness + helpers**; module docs state the live loop should call these helpers — **no end-to-end daemon process test** receiving real Telegram updates. |
| **RPC** | Outbound **`PresenterObserver`** / **`TelegramDaemonHooks`** exists for notifications; **no RPC surface** for interactive Telegram session control (plan approval, recipe selection) in `ConnectionService` or similar — integration tests use the harness directly, not gRPC. |
| **Config** | Existing **`telegram`** block (`enabled`, `bot_token`, `chat_ids`) supports **notifications**; there is **no separate config schema** documented for “session control allowlist” beyond reusing allowed chat ids in harness tests (`Vec<i64>` in code). |
| **PRD / `docs/ft/`** | **[telegram-notifications.md](../docs/ft/daemon/telegram-notifications.md)** describes **outbound** session notifications and Presenter stream elicitation — **not** an inbound command-and-control PRD for `/start-workflow`. Product docs for interactive Telegram-driven TDD workflow control would need to be added or cross-linked when the feature is wired. |

## Recommendations

1. **Before production wiring:** Add a **small unit test module** next to `InMemoryTelegramSender` verifying `send_message_with_inline_keyboard` + `recorded_with_keyboards` + `recorded()` invariants (so keyboard regressions surface without running full integration tests).
2. **Extend `telegram_session_control` unit tests** for `parse_demo_options_value` error paths and `chunk_telegram_text` boundary cases if you tighten behavior.
3. **Integration:** When the teloxide inbound loop lands, add **one** narrow daemon-level or black-box test (or documented manual checklist) that proves authorized chat id config matches harness assumptions.
4. **Optional CI:** Run `./dev cargo test --workspace --no-run` periodically or in CI if you need guaranteed compilation of all test targets across the workspace (this run used `cargo check --workspace` instead for a faster smoke test).

---

*Generated by validate-tests subagent for refactor validation.*

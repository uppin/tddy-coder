# Validate Tests Report

## Date / toolchain note

- **Date:** 2026-04-06
- **Toolchain:** `./dev` (Nix dev shell per repo; `rustc 1.94.0 (4a4ef493e 2026-03-02)`).
- **Note:** Cargo emitted `warning: Git tree '…' is dirty` (uncommitted changes); tests still completed successfully.

## Command run

```bash
cd /var/tddy/Code/tddy-coder/.worktrees/feature-telegram-concurrent-elicitation && ./dev cargo test -p tddy-daemon 2>&1
```

## Summary

| Metric | Value |
|--------|-------|
| **Overall** | **159 passed, 0 failed** (all `tddy-daemon` test targets) |
| Doc-tests | 0 |
| Exit code | 0 |

## Per-suite breakdown

Totals are taken from each binary’s `test result:` line.

| Suite / target | Passed |
|----------------|--------|
| Library (`src/lib.rs`) | 94 |
| Binary tests (`src/main.rs`) | 0 |
| `acceptance_daemon` | 8 |
| `delete_session` | 2 |
| `grpc_spawn_contract` | 1 |
| `list_agents_allowlist_acceptance` | 4 |
| `list_sessions_enriched` | 2 |
| `multi_host_acceptance` | 5 |
| `session_workflow_files_rpc` | 3 |
| `sessions_base_path_mismatch` | 1 |
| `signal_session` | 3 |
| `spawn_session_id_uuid_v7` | 2 |
| **`telegram_concurrent_elicitation_integration`** | **5** |
| `telegram_notifier` | 3 |
| `telegram_session_control_integration` | 20 |
| `worktrees_acceptance` | 2 |
| `worktrees_rpc` | 4 |
| Doc-tests | 0 |
| **Total** | **159** |

### Feature-focused integration tests (`telegram_concurrent_elicitation_integration`)

All five passed:

- `telegram_single_chat_two_sessions_second_prompt_is_queued_or_deferred`
- `telegram_active_session_token_routes_plain_text_answer_correctly`
- `telegram_callback_for_non_active_session_is_rejected_or_ignored_per_policy`
- `telegram_active_token_transfers_when_session_completes_elicitation`
- `telegram_regression_single_session_elicitation_still_works`

### Unit tests in `active_elicitation` (included in library 94)

From the run log, examples include: `first_registered_session_becomes_active_for_chat`, `primary_keyboard_suppressed_for_queued_session`, `callback_permitted_only_for_active_session_when_second_is_queued`, `advance_after_completion_promotes_next_queued_session`.

## Failed tests

**None.** No failing tests; no failure output to quote.

## `./test` at repo root (optional note)

`./test` differs from the command above in intentional ways:

- It **builds** `tddy-coder`, `tddy-tools`, `tddy-livekit`, and `tddy-acp-stub` (with `--examples --bins`) before running tests, then runs **`cargo test` for the whole workspace** (or filtered args), not only `-p tddy-daemon`.
- It forces **`--test-threads=1`** and tees output to `.verify-result.txt`.

So pass/fail **for `tddy-daemon` alone** should match this report when you run the same package filter (e.g. `./test -p tddy-daemon`), but **wall time and total test count** differ because `./test` includes other packages unless you pass `-p tddy-daemon`.

## Coverage gaps / recommendations

Relevant to **Telegram concurrent session elicitation** (`ActiveElicitationCoordinator`, notifier, session control harness, deferred keyboards, callback gating):

1. **`pending_elicitation_other` / multi-session “Other”** — Unit tests cover parsing (`parse_elicitation_other_callback_round_trip`). Consider an integration-style test where **two sessions** are queued and the user uses **“Other” / free-text** on the **non-active** session’s deferred surface (if ever exposed) or after promotion, to ensure routing and coordinator state stay consistent.
2. **Plain-text routing depth** — `telegram_active_session_token_routes_plain_text_answer_correctly` asserts the harness’s **active session** via `register_elicitation_surface_request`. A gap remains for **full inbound Telegram update → presenter** path (real `telegram_bot` handler + shared coordinator) if not already covered elsewhere end-to-end.
3. **Session teardown / queue cleanup** — Tests cover `advance_after_elicitation_completion` when the active session completes. Less explicit: **session deleted or disconnected** while queued or active (coordinator should not leave stale tokens or block the next session).
4. **Document review + active token** — `telegram_notifier` has document-review message tests; consider a **combined** scenario: document-review UI in one session vs select elicitation in another in the **same chat**, to validate ordering and which surface owns the token.
5. **Three or more sessions** — Queue behavior is covered for A then B; **A → B → C** promotion order and **duplicate `register`** idempotency are covered in unit tests; an integration test with **three** sessions could lock ordering under load.
6. **Outbound deferral** — `telegram_single_chat_two_sessions…` asserts at most one primary `eli:s:` keyboard from `TelegramSessionWatcher` with in-memory sender. Worth monitoring if **keyboard edit/remove** paths need explicit tests when the second prompt is collapsed vs fully deferred.

## Conclusion

`tddy-daemon` tests completed successfully: **159 passed, 0 failed** under `./dev cargo test -p tddy-daemon`. The dedicated **`telegram_concurrent_elicitation_integration`** suite (5 tests) passed alongside **library unit tests** for `active_elicitation` and existing Telegram notifier / session-control coverage.

Recommended follow-ups are **targeted integration or harness tests** for edge cases above (especially **teardown**, **document-review + concurrent elicitation**, and **deeper plain-text routing through the bot**), not because current tests failed—they did not—but to harden behavior that is only partially exercised by the current suite.

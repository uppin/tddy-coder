# Refactoring plan (synthesized from validate subagents)

**Date:** 2026-04-06  
**Scope:** Telegram concurrent elicitation (`tddy-daemon`)

## Preconditions

- `./dev cargo test -p tddy-daemon`: all passing (159 tests at last run).
- Remove or gitignore `packages/tddy-daemon/.red-test-output.txt` before merge.

## Priority 1 — Correctness / PRD alignment

1. **Queue advancement on all elicitation completion paths** — Today only `handle_elicitation_select` advances. Extend policy for Other follow-up completion, multi-select, document-review where appropriate, or document explicit exceptions.
2. **`pending_elicitation_other`** — Migrate or supplement with `active_elicitation` so concurrent sessions cannot overwrite chat→session mapping.
3. **Plain-text / command routing** — Resolve `/answer-text`, `/answer-multi`, and related paths via active token or explicit session id; fail closed when ambiguous.
4. **Document-review callbacks** — Decide whether `doc:*` actions use the same active-token gate as `eli:s:`/`eli:o:`; implement consistently in `telegram_bot.rs`.

## Priority 2 — Robustness

1. Replace bare `Mutex::unwrap()` on shared coordinator with poison handling or `map_err` where feasible.
2. Optional: cap per-chat queue length or log when queue exceeds N sessions.

## Priority 3 — Clean code

1. Extract shared helper in `telegram_callback_handler` for authorize + `elicitation_callback_permitted` + alert for elicitation callbacks.
2. Split `send_mode_changed_elicitation` into smaller private helpers (prepare state, chunks, action line).
3. Align naming: `advance_*` between coordinator and harness.

## Priority 4 — Tests

1. Integration tests for multi-session “Other” and plain-text routing once implemented.
2. Optional: one test with a single `SharedActiveElicitationCoordinator` wired to both watcher and harness.

## Priority 5 — Documentation

1. Operator docs (`docs/ft/daemon/`, changeset workflow) describing concurrent Telegram behavior.

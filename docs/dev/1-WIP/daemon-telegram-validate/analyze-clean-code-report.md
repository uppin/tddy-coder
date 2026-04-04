# Clean-code analysis: Telegram notifier (`tddy-daemon`)

**Scope:** Aligns with [evaluation-report.md](./evaluation-report.md) — library-level `telegram_notifier`, YAML config, tests; excludes main-loop wiring.

## Executive summary

The Telegram notifier code is **readable, test-backed, and reasonably layered**: pure helpers, a small stateful watcher, and a `TelegramSender` abstraction keep teloxide and orchestration apart. The main quality drag is **per-call trace scaffolding** (`marker_json` and M00x markers) that inflates noise and duplication without clear long-term value, plus **duplicated mock sender** types between unit and integration tests. Config changes are minimal and consistent with existing `deny_unknown_fields` patterns. Overall: **mergeable quality with targeted cleanup recommended** before treating this as finished product code.

## Strengths

- **Clear module documentation** (`telegram_notifier.rs` lines 1–6) and a focused doc comment on `on_metadata_tick` (`telegram_notifier.rs` lines 118–125) explain baseline-vs-transition behavior well.
- **`TelegramSender` trait** (`telegram_notifier.rs` lines 96–99) supports Dependency Inversion: production can use a teloxide-backed adapter; tests use mocks without network.
- **Structured logging** uses a stable target (`tddy_daemon::telegram`) and avoids printing raw tokens; `mask_bot_token_for_logs` (`telegram_notifier.rs` lines 56–67) encodes a clear security rule.
- **`TelegramSessionWatcher::on_metadata_tick`** (`telegram_notifier.rs` lines 126–220) has a linear control flow: config gate → active gate → baseline vs unchanged vs transition; easy to follow.
- **Config** (`config.rs` lines 52–54, 78–87): `telegram` is optional on `DaemonConfig`, `TelegramConfig` uses `deny_unknown_fields`, and fields match typical YAML usage.
- **Tests** cover label extraction, terminal recognition, masking, inactive sessions, disabled config, single send on transition, and no terminal spam (`telegram_notifier.rs` acceptance unit tests; `tests/telegram_notifier.rs`).

## Issues

| Area | Location | Notes |
|------|-----------|--------|
| Temporary / noisy instrumentation | `telegram_notifier.rs` 17–25, and call sites throughout | `marker_json` runs on almost every entry point; module comment says “reduced in later phases.” This reads like **non-production trace** and should be **FIXME/TODO** or removed per project conventions. |
| Duplication | `telegram_notifier.rs` 229–252 vs `tests/telegram_notifier.rs` 10–36 | Two near-duplicate `TelegramSender` mocks (`MockSender` vs `MockTelegramSender`) with slightly different capture semantics. |
| Trait documentation | `telegram_notifier.rs` 96–99 | `TelegramSender` and `send_message` lack doc comments (contract: `chat_id` semantics, error expectations). |
| Dead-ish diagnostic | `telegram_notifier.rs` 47–54, 182–185 | `is_terminal_session_status` is **only** used in a debug log line on the “unchanged status” branch, not to alter behavior. Name suggests stronger coupling to gating than exists; either use it in logic or rename/clarify. |
| Minor allocation / style | `telegram_notifier.rs` 35–36 | `session_telegram_label` builds a `Vec` from full split; `splitn` or two-prefix extraction would avoid collecting all segments. |
| Config ergonomics | `config.rs` 81–86 | `bot_token` is required whenever a `telegram:` block is deserialized; disabled configs must still supply a token string (see integration `telegram_disabled_config`). Acceptable but worth documenting if intentional. |
| Product visibility | `telegram_notifier.rs` 158–165, 207–216 | When `enabled` is true and `chat_ids` is empty, loops send nothing (aligned with [evaluation-report.md](./evaluation-report.md)); operators get limited signal unless log level captures `chat_targets=0`. |

## Refactor suggestions

1. **Remove or gate `marker_json`:** Replace with ordinary `trace!` at call sites, delete M00x IDs, or hide behind a single `cfg` if still needed for bring-up; mark any remaining temporary hooks with **FIXME**.
2. **Unify test doubles:** Extract a small `#[cfg(test)]` helper module in `telegram_notifier.rs` (or a `test_support` submodule) exporting one mock that can record either counts or `(chat_id, text)` vectors; use it from `tests/telegram_notifier.rs` via `tddy_daemon::telegram_notifier::...` re-exports if visibility allows, or accept one integration-only mock with a comment referencing the unit-test mock to reduce drift.
3. **Document `TelegramSender`:** One short paragraph on intended implementors (teloxide adapter vs tests) and whether `send_message` must be idempotent.
4. **Clarify `is_terminal_session_status`:** Either use it when deciding notifications (if product rules require) or rename to e.g. `status_looks_terminal_for_logging` and keep only in logs — avoid implying unused policy.
5. **`session_telegram_label`:** Prefer `splitn(3, '-')` or manual indexing for the two-segment rule without allocating a `Vec` for long session ids.
6. **Optional:** When `tg.enabled && tg.chat_ids.is_empty()`, log at `warn!` once per process or per tick (product decision) to match evaluation-report guidance.

## SOLID (brief)

- **S:** `TelegramSessionWatcher` owns transition state; helpers are free functions — good separation.
- **O:** New send backends implement `TelegramSender` without editing the watcher.
- **L:** Mocks substitute for real sender — appropriate for tests.
- **I:** `TelegramSender` is a minimal interface (`send_message` only) — good.
- **D:** Watcher depends on `TelegramSender` and `DaemonConfig`, not on `Bot` directly — good; `send_telegram_via_teloxide` is the concrete teloxide edge.

## Consistency with repo Rust style

- Uses `anyhow::Result`, `async_trait`, `log` with targets — consistent with typical daemon patterns in this workspace.
- Module-level `//!` and public API docs on key methods match common Rust style; trait could use the same level of documentation.
- Tests named `acceptance_unit_tests` and integration tests in `tests/` mirror common layout; duplicate mocks are the main inconsistency.

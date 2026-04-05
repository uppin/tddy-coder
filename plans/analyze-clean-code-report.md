# Clean-code analysis: Telegram session control

**Scope:** `packages/tddy-daemon/src/telegram_session_control.rs`, relevant parts of `packages/tddy-daemon/src/telegram_notifier.rs`, `packages/tddy-daemon/tests/telegram_session_control_integration.rs`  
**Tooling:** `./dev cargo fmt -p tddy-daemon -- --check` — **passed** (exit 0).

---

## Summary

The new control-plane module is readable, well-structured with section headers, and backed by a clear `TelegramSender` abstraction from `telegram_notifier`. Documentation at module and public-type level is generally strong. Main improvement areas are **naming vs. behavior** (`parse_callback_payload`), **incomplete use of parameters** in plan review, **tight coupling** to `InMemoryTelegramSender` for the harness, and **mixed responsibilities** inside `TelegramSessionControlHarness`.

---

## Strengths

### `telegram_session_control.rs`

- **Module-level docs** state purpose, bridge to web encodings, and point to the harness and tests.
- **Public contract types** (`StartWorkflowCommand`, `TelegramCallback`, `CapturedTelegramMessage`, outcomes, `PresenterInputPayload`, `WorkflowTransitionKind`) are documented and used consistently in tests.
- **Separation of pure helpers** (`parse_start_workflow_prompt`, `chunk_telegram_text`, `map_elicitation_callback_to_presenter_input`) from I/O-heavy harness methods improves testability; inline `#[cfg(test)]` unit tests cover parsers and chunking.
- **Logging** uses a dedicated target (`tddy_daemon::telegram_session_control`) for filterability.
- **`chunk_telegram_text`** documents edge cases (empty input, `max_utf8_bytes == 0`, continuation suffix vs. byte-only splits) and UTF-8 boundary handling via `take_utf8_prefix`.

### `telegram_notifier.rs` (relevant sections)

- **`TelegramSender`** is a small, focused async trait — good boundary for dependency inversion; production (`TeloxideSender`) and test (`InMemoryTelegramSender`) implementations stay behind the trait for plain sends.
- **`InMemoryTelegramSender`** extension (`recorded_with_keyboards`, `send_message_with_inline_keyboard`) is documented as optional for keyboard-aware tests; `recorded()` remains backward compatible.
- **Docs on `InMemoryTelegramSender`** and **`send_daemon_lifecycle_message`** clarify intent.

### `telegram_session_control_integration.rs`

- **Stable chat constants** (`AUTHORIZED_CHAT`, `UNAUTHORIZED_CHAT`) avoid magic numbers.
- **`harness_with_sender`** centralizes harness construction and shared `Arc<InMemoryTelegramSender>`.
- **Tests map to user-visible scenarios** (start workflow + keyboard, recipe callback → YAML, plan chunking + transition, elicitation bytes, unauthorized denial).
- **Assertions** include actionable failure messages (`sent={sent:?}`, `recorded={:?}`).

---

## Issues

### Naming and semantics

| Item | Concern |
|------|--------|
| `parse_callback_payload` | Returns `Some` only when `callback_data.contains("recipe:")`. The name implies generic callback parsing; actual behavior is recipe-specific. Callers may assume other callback types are recognized. |
| `parse_demo_options_value` | Heuristic `replace(":true", ": true")` is brittle for nested structures or keys containing those substrings; acceptable for tests but worth documenting as a **known limitation** if kept. |

### Function complexity and completeness

- **`chunk_telegram_text`**: Two main branches (with vs. without continuation room). Complexity is moderate and localized; acceptable, but the `max_utf8_bytes == 0` branch returns a single full string — behavior is documented; ensure product callers never rely on ambiguous edge cases.
- **`handle_plan_review_phase`**: Takes `approval_callback` but ends with `let _ = approval_callback;` — the approval payload is **not** used to validate or branch. That reads as **unfinished behavior** or misleading API (violates principle of least surprise). Either integrate approval handling or narrow the signature / document as reserved for future use with a `TODO`/`FIXME` per project rules.
- **`handle_recipe_callback`**: Mixes parsing, YAML load/merge, and write — still readable; could grow if more segments are added.

### Duplication

- Repeated **`log::info!` / `log::debug!`** blocks with the same target across methods — could extract small helpers (e.g. `fn log_control(level, msg: ...)`) if churn increases; not urgent.
- **`TelegramSessionWatcher`-style** “enabled + chat_ids loop” patterns exist elsewhere in `telegram_notifier`; session control uses `InMemoryTelegramSender` directly — duplication is low between files, but **broadcast-to-chats** logic is conceptually similar.

### SOLID / module boundaries

- **`TelegramSessionControlHarness`**: Combines authorization, filesystem (session dir, `changeset.yaml`), and Telegram outbound calls. For a harness this is **acceptable**; for production extraction, consider splitting **authorization**, **changeset persistence**, and **messaging** to respect single responsibility and ease reuse from a future teloxide dispatcher.
- **Dependency direction**: `telegram_session_control` imports `InMemoryTelegramSender` and `TelegramSender` from `telegram_notifier` — reasonable; the harness is test-oriented. Production wiring should keep **domain types** in `telegram_session_control` and **transport** in notifier/teloxide layers.
- **`TelegramSender` trait** does not include `send_message_with_inline_keyboard`; the harness calls that method on the concrete `InMemoryTelegramSender`. That **leaks** a test-only API into the harness type — fine for tests, but if production needs keyboards, the trait or a separate abstraction should be extended consistently.

### Documentation (module docs, public API)

- **Strong** for top-level types and chunking/elicitation encoding.
- **Gaps**: `parse_callback_payload` should document the **exact** subset of callbacks handled. `ChangesetRoutingSnapshot` fields `demo_options` / `run_optional_step_x` — clarify whether Telegram path populates all fields or only those under test.
- **`handle_start_workflow_unauthorized`**: Docs say unauthorized behavior; the early return `Ok(None)` when chat **is** authorized is correct but subtle — worth one line in doc (“returns `None` if chat is authorized — caller should use `handle_start_workflow`”).

### Test quality

- **Integration file** includes a **pure encoding test** (`telegram_elicitation_choice_mapped_to_presenter_expected_input`) that duplicates coverage already in unit tests in `telegram_session_control.rs`. Acceptable as acceptance redundancy; optional consolidation to avoid double maintenance.
- **`unwrap()` on `tempfile::tempdir()`** — standard in tests; fine.
- **Integration tests** do not assert on **filesystem session persistence** for `handle_start_workflow` (only session id + outbound messages) — may be intentional; call out if disk layout is part of the contract.

---

## Prioritized refactors

1. **High — API honesty:** Resolve `approval_callback` in `handle_plan_review_phase` (use it or remove/rename parameters) and align `parse_callback_payload` name/docs with behavior (or implement real multi-type parsing).
2. **Medium — Naming/docs:** Rename or narrowly document `parse_callback_payload`; document `parse_demo_options_value` limitations or replace with a stricter parser if production-bound.
3. **Medium — Boundaries:** If teloxide integration lands, introduce a small **outbound port** (trait) for “plain text + optional inline keyboard” so the harness and production share one abstraction instead of concrete `InMemoryTelegramSender` methods.
4. **Low — DRY:** Optional shared logging helper for `telegram_session_control` to shrink repetitive log blocks.
5. **Low — Tests:** Drop duplicate elicitation byte test from integration or mark it explicitly as acceptance-only duplicate; add an integration assertion for **session directory** contents after start if that becomes a guaranteed contract.

---

## `rustfmt`

`./dev cargo fmt -p tddy-daemon -- --check` completed successfully — formatting for this package matches the toolchain’s expectations.

# Clean Code Analysis Report

## Overall assessment

The concurrent elicitation work introduces a small, well-bounded coordination module (`active_elicitation.rs`) and wires it through outbound notifications and inbound session control with a clear “one active token per Telegram chat” policy. Naming and module documentation generally match the PRD intent. The main quality gaps are **size/complexity** in `send_mode_changed_elicitation`, **repeated callback-guard patterns** in `telegram_bot.rs`, and **naming asymmetry** between the coordinator and harness APIs. Logging mostly follows existing daemon conventions (`tddy_daemon::telegram`, `tddy_daemon::telegram_session_control`, `tddy_daemon::telegram_bot`), with a dedicated target for the new coordinator (`tddy_daemon::active_elicitation`), which is reasonable for grepability.

## Strengths

- **Single place for queue semantics**: `ActiveElicitationCoordinator` owns per-`chat_id` FIFO queues, duplicate registration, head mismatch handling, and drain-on-empty. Outbound code calls `should_emit_primary_elicitation_keyboard` instead of inlining comparisons.
- **Clear dependency direction**: `TelegramSessionWatcher` and `TelegramSessionControlHarness` both depend on a shared `SharedActiveElicitationCoordinator` (Arc), not on each other — aligns with Dependency Inversion for the “policy” object.
- **Harness surface for tests**: Public methods (`active_elicitation_session_for_chat`, `register_elicitation_surface_request`, `elicitation_callback_permitted`, `advance_elicitation_queue_after_completion`) give integration tests a stable contract without reaching into internals.
- **Module docs**: `active_elicitation.rs` and the top of `telegram_notifier.rs` explain cross-module wiring and point to the integration test file.
- **Unit coverage in `active_elicitation`**: Tests cover active session, queued callback denial, promotion after completion, and primary keyboard gating.
- **User-facing policy in the bot**: Non-active elicitation callbacks are rejected with an alert and a consistent message, plus `info!` logs for observability — good operational behavior.

## Issues

### Info

- **Naming asymmetry**: The coordinator exposes `advance_after_elicitation_completion` while the harness exposes `advance_elicitation_queue_after_completion`. Both are accurate but slightly harder to navigate when reading call chains; consider aligning naming (e.g. both `advance_*_after_elicitation_completion` or both `*_queue_*`).
- **`send_mode_changed_elicitation` size**: One method handles signature dedupe, option cache updates, registration per chat, document body chunking, clarification chunking, and primary vs deferred keyboard send paths. It is readable but at the upper bound for maintainability.
- **`marker_json` / M00x trace hooks** in `telegram_notifier.rs`: Documented as development tracing; they fire on many code paths. If retained long-term, consider whether volume is acceptable or should be consolidated behind a single debug gate.
- **Mutex poisoning**: New code uses `.lock().unwrap()` on `SharedActiveElicitationCoordinator` in several places (consistent with nearby patterns but inconsistent with `map_err` used elsewhere for other mutexes).

### Warning

- **Duplicated callback guard in `telegram_bot.rs`**: `parse_elicitation_other_callback` and `parse_elicitation_select_callback` branches repeat the same structure: lock harness → authorize → `elicitation_callback_permitted` → optional alert → `answer_callback_query` → second lock for handler. This risks drift if one path is updated and the other is not.
- **Integration test vs production wiring**: `telegram_concurrent_elicitation_integration.rs` uses `TelegramSessionWatcher::new()` (default coordinator) for the “at most one primary keyboard” test and a standalone `TelegramSessionControlHarness` without a shared coordinator for harness-only tests. That matches the stated scenarios but means the **full** shared-Arc path is exercised by production wiring and unit tests more than by this integration file — worth documenting or adding one test that shares the coordinator between watcher and harness if end-to-end invariants matter.

## Suggested refactors (prioritized)

1. **Extract a small helper in `telegram_callback_handler`** for “authorized + elicitation permitted + alert or proceed” for `eli:o:` / `eli:s:` callbacks to remove duplication and keep alert text in one place.
2. **Split `send_mode_changed_elicitation`** into private helpers: e.g. `prepare_elicitation_state` (signature, cache, register), `send_mode_changed_chunks`, `send_mode_changed_action_line` (primary vs deferred). Keeps behavior identical while shrinking the main function.
3. **Align public method names** between `ActiveElicitationCoordinator` and `TelegramSessionControlHarness` for the advance operation (pure rename / thin wrapper), to reduce cognitive load when grepping.
4. **Optional**: Add one integration-style test (or extend the existing file) that constructs **one** `SharedActiveElicitationCoordinator` passed to both `TelegramSessionWatcher::with_elicitation_select_options_and_coordinator` and `TelegramSessionControlHarness::with_workflow_spawn` (or equivalent) so the shared queue is validated under a single test harness — only if product requirements demand that level of assurance.

## Conclusion

The feature is **structurally sound**: queue and token rules live in one coordinator, outbound and inbound paths share state explicitly, and the bot enforces the policy at the callback boundary. The highest-impact cleanups are **reducing duplication in the Telegram callback handler** and **trimming `send_mode_changed_elicitation`** without changing behavior. Naming alignment between coordinator and harness is a low-cost polish. Overall the code is consistent with existing daemon logging patterns and reads as production-oriented rather than ad hoc.

# Changeset: Telegram session control library (State B)

**Date**: 2026-04-05  
**Status**: PR-ready (PR-wrap: fmt, workspace clippy, full `./test` green)  
**Type**: Feature (daemon library + product documentation + related crates)

## Affected areas

- `docs/ft/daemon/` — **`telegram-session-control.md`**, **`telegram-notifications.md`**, **`changelog.md`** (2026-04-05: document body + Approve/Reject/Refine requirement, Telegram entities)
- `docs/dev/` — **`changesets.md`**, this WIP file
- `packages/tddy-daemon/src/` — **`telegram_session_control.rs`**, **`telegram_notifier.rs`**, **`telegram_bot.rs`**, **`presenter_intent_client.rs`**, **`main.rs`**, **`connection_service.rs`**, **`elicitation.rs`**, spawn path, **`tests/`**
- `packages/tddy-service/` — **`presenter_intent.proto`**, **`PresenterIntent`** service, **`remote.proto`** / codegen as applicable
- `packages/tddy-core/` — presenter **`intent`** wiring for workflow/daemon alignment
- `packages/tddy-coder/` — child gRPC / session startup where touched by this branch
- `packages/tddy-web/` — generated protos / terminal components if included in branch
- `packages/tddy-e2e/tests/grpc_reconnect_acceptance.rs` — **compile fix**: assertion preview used `floor_char_boundary` on `Vec<u8>`; use UTF-8 lossy preview on `str` (unblocks workspace `cargo test` / `./test`).

## Summary

The **`telegram_session_control`** module provides inbound-oriented parsers, chunking, presenter-byte mapping, and a test harness that writes **`changeset.yaml`** and uses **`InMemoryTelegramSender`** with optional inline keyboard capture. Outbound session notifications remain in **`telegram_notifier`**. Feature documentation describes the split and the harness contract without delta phrasing.

## PR-wrap validation (Updated: 2026-04-05)

### `/validate-changes` (risk / scope)

- **Inbound Telegram + localhost presenter gRPC**: Intended boundary; token/session resolution remains in existing daemon patterns. Reviewers should confirm **`elicitation_select_options`** cache stays consistent with **`session_id`** used in **`ModeChanged`** vs callback **`eli:s:`** payloads (same UUID string).
- **Cross-package surface**: Proto + web/tsgen + core + coder touches are coupled; PR description should list them for reviewers.
- **No secrets**: Bot tokens and paths remain config-driven; logs continue to mask tokens where applicable.

### `/validate-tests` (quality)

- Daemon integration tests use **`InMemoryTelegramSender`** and temp dirs; no conditional test logic added in this wrap.
- Full workspace **`./test`** is the acceptance bar for merge (includes **`tddy-daemon`** integration and dependent crates).

### `/validate-prod-ready`

- Outstanding **FIXME/TODO** markers called out in this file (plan review callback, prompt persistence) remain **product gaps**, not merge blockers for the Telegram session-control slice unless the team decides otherwise.

### `/analyze-clean-code`

- **Workspace** `cargo clippy --workspace -- -D warnings` — pass after prior refactors.
- Incidental fix: **`grpc_reconnect_acceptance`** assertion preview — `Vec<u8>` is not UTF-8 `str`; use **`String::from_utf8_lossy`** + **`str::floor_char_boundary`** for a safe debug prefix.

### Refactoring applied (historical)

- Renamed `drain_outbound_messages` → `collect_outbound_messages` (name matched behavior)
- Elided needless lifetimes on `take_utf8_prefix` (clippy)
- Extracted `RecordedMessage` type alias in `InMemoryTelegramSender` (clippy type-complexity)
- Added FIXME for `handle_plan_review_phase` approval callback (not yet wired)
- Added TODO for `handle_start_workflow` prompt persistence

### Test additions (9 → 15 telegram tests)

- `parse_start_workflow_returns_none_for_unrecognized_command` (unit)
- `parse_callback_payload_returns_none_for_non_recipe_data` (unit)
- `chunk_telegram_text_empty_input_returns_single_empty_chunk` (unit)
- `chunk_telegram_text_zero_max_returns_full_text` (unit)
- `chunk_telegram_text_respects_utf8_boundaries` (unit)
- `telegram_authorized_chat_returns_none_from_unauthorized_handler` (integration)
- Strengthened `demo_options` assertion to verify concrete `run: true` value
- Added `session_dir.exists()` assertion for start workflow

### Quality gates (PR-wrap final)

- `cargo fmt --all` — pass
- `cargo clippy --workspace -- -D warnings` — pass
- `./test` (repo script: `tddy-coder` + `tddy-tools` build, full `cargo test` workspace) — **pass** (exit 0)

## Requirements update (Updated: 2026-04-05)

New Telegram commands added to feature doc scope (not yet implemented):

- **`/sessions`** — List sessions 10 at a time with "More" pagination button; each entry has "Enter" and "Delete" inline keyboard buttons.
- **`/delete <session_id>`** — Delete a session (delegates to `session_deletion::delete_session_directory`).
- **Enter workflow** — Connect to a session's workflow from the session list; present current state and available actions.

These extend the existing `/start-workflow` command. Implementation will reuse `session_reader::list_sessions_in_dir`, `session_list_enrichment`, and `session_deletion::delete_session_directory` from the daemon's `ConnectionService` pipeline.

### Document body + actions on Telegram (Updated: 2026-04-05)

**Requirement:** When a document is presented for review / approve / reject, **send the document contents to Telegram** (chunked if needed), **then** send **Approve / Reject / Refine** inline buttons (aligned with Virtual TUI semantics).

**Technical notes (Telegram formatting):**

- Telegram entity **offsets and lengths** use **UTF-16 code units** ([Styled text with message entities](https://core.telegram.org/api/entities)); supplementary-plane characters count as two units. Any markdown→Telegram conversion or manual `MessageEntity` construction must compute spans using that rule (or use Bot API `parse_mode` with validated output).
- Reuse **`chunk_telegram_text`** (or equivalent) for oversized bodies so sends stay within Telegram message limits; the action keyboard attaches to the **final** segment or a dedicated short follow-up message (product choice: see feature doc).
- Keep **dedupe** semantics for `ModeChanged` compatible with sending multi-message document bodies (dedupe key may need to include “document sent” vs “hint only” or be scoped so retries do not duplicate full text).

### From @red (TDD Red Phase) (Updated: 2026-04-05)

Failing tests in **`telegram_notifier`** acceptance module define the green-phase contract:

- [ ] Send **`AppModeDocumentReview` / `AppModeMarkdownViewer` `content`** via Telegram (chunked when over the Bot API limit), then a short action line with an inline keyboard.
- [ ] Keyboard row must expose **Approve**, **Reject**, and **Refine** (product alignment with Virtual TUI; may replace **View** or add **Reject** alongside existing callbacks).
- [ ] Preserve **`telegram_notifier_dedupes_repeated_identical_elicitation_signals`** behavior: duplicate identical `ModeChanged` must not append sends (works with multi-message first delivery).

## References

- [telegram-session-control.md](../../ft/daemon/telegram-session-control.md)
- [telegram-notifications.md](../../ft/daemon/telegram-notifications.md)
- [daemon changelog](../../ft/daemon/changelog.md)

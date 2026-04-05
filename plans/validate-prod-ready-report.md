# Production readiness: Telegram session control

**Scope:** `packages/tddy-daemon/src/telegram_session_control.rs`, `InMemoryTelegramSender` changes in `telegram_notifier.rs`, `tests/telegram_session_control_integration.rs`  
**Review date:** 2026-04-05

## Executive summary

The new code establishes a **testable contract** for Telegram-driven workflow control (parsing, chunking, presenter byte encoding, and a **`TelegramSessionControlHarness`** with chat allowlisting). Logging uses explicit `log` targets and generally avoids sensitive content (lengths and paths, not prompts or tokens).

**Production readiness is not met** for an end-to-end Telegram control plane: the harness is **not wired** into the daemon’s teloxide inbound path or `DaemonConfig` in `main.rs`—only outbound notifications (`TelegramSessionWatcher` / `TelegramDaemonHooks`) are. Several behaviors are **explicitly test-scoped** (e.g. plan chunk size `CHUNK_MAX = 24`), and authorization is **chat-only** with no binding between Telegram identity and session directories for callbacks.

**Overall risk level:** **Medium–High** until inbound integration, config alignment, production chunk limits, and session–chat binding are addressed.

## Risk level

| Area | Level | Note |
|------|--------|------|
| Security (authz, session binding) | **High** | Allowlist exists; no user verification; callbacks not tied to owning session/chat. |
| Configuration / feature flags | **High** | No `DaemonConfig` fields or flags for inbound control; harness uses ad-hoc `Vec<i64>`. |
| Error handling / data integrity | **Medium** | Silent empty `changeset.yaml`; YAML parse failures propagate; some parsers are loose. |
| Logging / secrets | **Low** | Targets and length-based fields are sound; tokens not present in this module. |
| Performance | **Low–Medium** | Acceptable for expected scale; chunking allocates; sequential sends. |

## Checklist

| Criterion | Status | Notes |
|-----------|--------|--------|
| Errors surfaced with `anyhow` / `Result` on I/O and YAML | **Pass** | `handle_*` and `read_changeset_routing_snapshot` return `anyhow::Result`. |
| Edge cases: empty input, UTF-8 boundaries | **Partial** | `chunk_telegram_text` handles empty and char boundaries; `max_utf8_bytes == 0` returns full string (documented). |
| Edge cases: missing/malformed `changeset.yaml` on recipe callback | **Fail** | `read_to_string` + `unwrap_or_default()` treats missing file as empty mapping—risk of silent overwrite. |
| Logging: `log` crate, stable targets | **Pass** | `tddy_daemon::telegram_session_control` and `tddy_daemon::telegram` for in-memory sender. |
| No secrets in logs (tokens, prompts) | **Pass** | Lengths, `chat_id`, path display; no bot token logging in scoped files. |
| Configuration: `DaemonConfig` / YAML for control plane | **Fail** | Harness takes constructor args; not loaded from `TelegramConfig`; no separate inbound flag. |
| Security: chat allowlist | **Partial** | `ensure_authorized` for `chat_id`; `user_id` on commands is logged but **not** checked. |
| Security: session scoping for callbacks | **Fail** | `handle_recipe_callback` accepts any `session_dir` without proving it belongs to the chat/session. |
| Production Telegram message limits | **Fail** | `handle_plan_review_phase` uses `CHUNK_MAX = 24` (test forcing); not ~4096 UTF-16/codepoint policy. |
| Async: no blocking in wrong places | **Pass** | Harness methods are `async`; I/O is sync in async fns (acceptable at small scale; see recommendations). |
| Integration tests cover critical paths | **Partial** | Start workflow, recipe write, plan chunks, unauthorized message, elicitation bytes; no negative paths for YAML/recipe. |

## Findings

### Error handling (`anyhow`, edge cases)

- **Strengths:** I/O and YAML errors propagate from `handle_recipe_callback`, `handle_start_workflow`, and `read_changeset_routing_snapshot`.
- **`handle_recipe_callback`:** `std::fs::read_to_string(&path).unwrap_or_default()` collapses “file missing” into an empty document, then writes—can **create or replace** content without an explicit decision. Prefer `read_to_string` and map `NotFound` to a clear error, or require an existing file for updates.
- **`parse_callback_payload`:** Returns `Some` only when the string contains the substring `"recipe:"`—brittle and not a structured parse.
- **`map_elicitation_callback_to_presenter_input`:** If the payload lacks the `elicitation:` prefix, the code still encodes using `unwrap_or(callback_data)`—risk of **mis-encoding** arbitrary strings as presenter input.
- **`parse_demo_options_value`:** Heuristic `:true`/`:false` spacing fix is fragile for real YAML edge cases.

### Logging (`log`, targets, secrets)

- **Strengths:** Consistent `target: "tddy_daemon::telegram_session_control"` for the control module; debug/info mix; `InMemoryTelegramSender::send_message_with_inline_keyboard` logs `chat_id`, `text_len`, `keyboard_rows` only.
- **Note:** `handle_start_workflow` logs `user_id` (not secret, but PII-adjacent)—acceptable for ops if retention policies allow.

### Configuration (`DaemonConfig`, feature flags)

- **`TelegramConfig`** (`config.rs`) exposes `enabled`, `bot_token`, `chat_ids` for **notifications**, not for labeling this inbound harness.
- **Gap:** No field such as `telegram_session_control_enabled`, no reuse of `chat_ids` as the allowlist for the harness, and no documented merge rule if notification chats differ from control chats.
- **Integration:** `main.rs` only constructs `TelegramDaemonHooks` (outbound presenter observer). **`TelegramSessionControlHarness` is not registered**—inbound control is **not production-active**.

### Security (authorization, allowlist, tokens)

- **Allowlist:** `ensure_authorized` checks `chat_id` against `allowed_chat_ids`.
- **Gaps:**
  - **`user_id` is not authorized**—any member of an allowed group chat could act; no admin vs member distinction.
  - **Callback path safety:** `handle_recipe_callback(&session_dir, cb)` does not validate that `session_dir` is the one associated with this chat’s prior `handle_start_workflow`—callers must enforce; the API does not.
  - **Token logging:** Not applicable in these files; production teloxide path should continue using patterns like `mask_bot_token_for_logs` elsewhere (already present in `telegram_notifier.rs`).

### Performance (allocations, chunk sizes, async)

- **`chunk_telegram_text`:** Per-chunk `format!` / `to_string` allocations; acceptable for typical plan sizes.
- **`handle_plan_review_phase`:** Sequential `send_message` in a loop—appropriate for Telegram rate limits; consider batching only if API allows and limits are configured.
- **`CHUNK_MAX = 24`:** Favors integration tests (forced continuation markers), **not** production limits—must be config-driven and aligned with Telegram’s limits (and encoding: UTF-16 length for Bot API in some cases).
- **`InMemoryTelegramSender::recorded_with_keyboards`:** Full `clone()` of stored messages—fine for tests; unbounded growth if used as a long-lived fake without clearing.

### Tests (`telegram_session_control_integration.rs`)

- **Strengths:** Covers keyboard on start, changeset persistence, chunked plan delivery, unauthorized denial, elicitation byte mapping.
- **Gaps:** No test for corrupt `changeset.yaml`, missing file behavior on recipe callback, or unauthorized `handle_recipe_callback` / `handle_plan_review_phase` (only unauthorized entry path via `handle_start_workflow_unauthorized`).

## Recommendations for follow-up

1. **Wire inbound teloxide updates** to shared helpers (not only the harness), with a single place that enforces allowlist + session correlation.
2. **Add `DaemonConfig` (or nested) options:** inbound enable flag, allowlist (or explicit reuse of `telegram.chat_ids` with documented semantics), and **production chunk size** (bytes or policy enum).
3. **Replace `CHUNK_MAX = 24`** with a constant or config default matching Telegram limits; keep test-only small values only in tests via injected limits.
4. **Harden `handle_recipe_callback`:** fail closed on missing `changeset.yaml` if updates are not intended to create; or document “create if absent” and add tests.
5. **Tighten `map_elicitation_callback_to_presenter_input`:** require `elicitation:` prefix or return `Result` / explicit enum instead of silent fallback.
6. **Session–chat binding:** persist mapping `(session_id, chat_id)` and validate on every callback referencing `session_dir` or `session_id`.
7. **Optional:** consider `user_id` checks if the bot only serves private chats with a known operator set.
8. **Async I/O:** for large files or high concurrency, move blocking `std::fs` off the runtime with `spawn_blocking` or use async fs (project-wide convention permitting).

---

*This review is limited to the files listed in the request; production behavior also depends on future teloxide wiring and `ConnectionService` integration not fully covered here.*

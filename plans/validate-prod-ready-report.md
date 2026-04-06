# Production Readiness Report

**Scope:** Telegram concurrent elicitation in `packages/tddy-daemon` (`active_elicitation`, `telegram_notifier`, `telegram_session_control`, `telegram_bot`, `main` wiring).  
**Date:** 2026-04-06

## Executive summary

**Overall risk: Medium**

The shared `ActiveElicitationCoordinator` and outbound deferral of non-primary `eli:s:` keyboards are directionally correct for “one visible interactive prompt per chat.” Inbound gating for `eli:s:` / `eli:o:` matches that policy with clear user alerts. However, several completion paths never advance the per-chat queue, document-review inbound actions are not subject to the same active-session gate, and unbounded queue growth plus `unwrap()` on mutexes remain production risks. Treat merge as **medium risk** until queue advancement and document-review policy are aligned with the PRD for all elicitation surfaces.

---

## Findings by category

### 1. Error handling: panics (`unwrap`), recoverable errors, user-facing messages

- **`std::sync::Mutex::lock().unwrap()`** on `active_elicitation` and related caches will **panic** if the mutex is poisoned (e.g. after a panic in another thread). Examples: `telegram_notifier.rs` (e.g. lines 506–516, 551–554), `telegram_session_control.rs` (e.g. lines 771–774, 1184–1185), `active_elicitation` callers throughout.
- **`InMemoryTelegramSender`** uses `.expect("InMemoryTelegramSender mutex")` — acceptable for tests only; not used on the production teloxide path.
- **Harness methods** generally return `anyhow::Result` for I/O and gRPC errors; **telegram_bot** surfaces failures with `format!("{e:#}")` to the user (e.g. `telegram_bot.rs` lines 72–74, 99–101, 297–299, 331–333, 368–369), which can expose internal error chains (paths, low-level reasons). Consider sanitizing or mapping to stable operator messages for production.
- **`resolve_child_grpc_port`** uses `hits.pop().expect("len 1")` (`telegram_session_control.rs` line 339) — logically unreachable if `hits.len() == 1`, but still a panic path in production code.
- **Integration test artifact** `.red-test-output.txt` suggests prior failing `active_elicitation` unit tests (marker / logging side effects). Verify current `cargo test -p tddy-daemon` before merge; do not treat red output as acceptable without fixing root cause.

### 2. Logging: levels, PII/secrets, queue observability

- **Secrets:** `mask_bot_token_for_logs` (`telegram_notifier.rs` lines 73–84) documents intent not to log raw tokens; good pattern. Bot token is not logged in snippets reviewed.
- **Identifiers:** `session_id` (full UUID) and `chat_id` are logged at **info** on transitions (e.g. `active_elicitation.rs` lines 40–47, `telegram_notifier.rs` `send_mode_changed_elicitation`, `telegram_session_control.rs` `handle_elicitation_select`). Acceptable for ops; be aware of **correlation / user identification** in shared log sinks.
- **Queue rotation:** `advance_after_elicitation_completion` logs **head mismatch**, **new active session**, and **queue drained** (`active_elicitation.rs` lines 73–108). `handle_elicitation_select` logs completion and next active (`telegram_session_control.rs` lines 1186–1192). Good baseline for debugging rotation; **no structured metric** (counters/histograms) — logs only.
- **Debug noise:** `marker_json` / trace markers (`telegram_notifier.rs` lines 35–41, 48–51, etc.) run on hot paths like `session_telegram_label`; cost is small but **debug-level** volume can grow in busy chats.
- **Deferred keyboard:** Info-level log when deferring (`telegram_notifier.rs` lines 568–573) aids “why no keyboard” investigations.

### 3. Configuration: env/YAML; multi-session

- **Telegram** remains **`config.telegram`** (`enabled`, `bot_token`, `chat_ids`) — no new YAML keys for concurrent elicitation (`main.rs` lines 167–224). Multi-session behavior is **purely logical** (per-chat queue), not separately configurable (no max queue depth, no feature flag).
- **Env overrides** in `main.rs` (`apply_env_overrides`) do not add Telegram-specific vars in the shown snippet; existing `config.apply_telegram_env_overrides()` applies as before.
- **Gap:** No configuration for **max queue length**, **TTL**, or **back-pressure** if many sessions enqueue elicitation for one chat.

### 4. Security: `callback_data` limits, authorization before elicitation handlers

- **64-byte limit:** `clarification_select_keyboard` checks length and **skips** overlong options / Other with `log::warn!` (`telegram_notifier.rs` lines 819–847). Risk: **incomplete keyboards** without failing the send; user may not see all choices.
- **Document review:** `doc:<action>:<session_id>` (`telegram_notifier.rs` lines 705–707) — length should fit typical UUIDs; validate if session ids ever exceed assumptions.
- **Authorization:** `ensure_authorized` / `is_authorized` against configured `chat_ids` is applied on harness entry points. **telegram_bot** checks authorization before session/recipe/elicitation handlers (e.g. `telegram_bot.rs` lines 180–186, 283–291, 303–327, 337–361).
- **Active-session gate:** **`eli:s:` and `eli:o:`** callbacks check `elicitation_callback_permitted` (`telegram_bot.rs` lines 311–327, 345–361) and show an alert if not active — **good**.
- **Gap — document review:** `parse_document_review_callback` path **does not** call `elicitation_callback_permitted` (`telegram_bot.rs` lines 283–300). Any in-chat **historical** `doc:*` message could still invoke `handle_document_review_action` for that `session_id` if the child gRPC port still resolves. This weakens “single active elicitation” for document approval relative to select/Other.

### 5. Performance: lock contention, per-chat queue growth

- **`Arc<StdMutex<ActiveElicitationCoordinator>>`** is held briefly for register / advance / checks, but **nested use** in `send_mode_changed_elicitation` (register then later `should_emit_primary_elicitation_keyboard`) takes **two separate locks** (`telegram_notifier.rs` lines 512–516 vs 549–554) — minor contention, not a long hold.
- **Broader contention:** Same coordinator is touched from **async Telegram path** and **watcher / gRPC observer** contexts; `StdMutex` blocks the executor if held across `.await` — current code appears to **not** hold the elicitation mutex across await (lock/drop in sync blocks). **Verify** no future refactor holds `active_elicitation` across `.await`.
- **Queue growth:** `queues: HashMap<i64, Vec<String>>` has **no cap** (`active_elicitation.rs` lines 17–19). Many concurrent sessions in one chat → **unbounded `Vec`**, memory growth, and many deferred text notices.
- **`pending_elicitation_other`:** One `session_key` per `chat_id` (`telegram_session_control.rs` lines 628–629, 1230–1233) — cannot stack multiple “Other” awaits; consistent with single active prompt if gates are correct.

### 6. Gaps vs PRD: `pending_elicitation_other`, plain-text routing, document-review concurrency

- **`pending_elicitation_other`:** Implemented (`TelegramWorkflowSpawn.pending_elicitation_other`, `handle_elicitation_other`, `handle_elicitation_other_followup_plain_message`). **Missing:** after a **successful** Other follow-up (`telegram_session_control.rs` lines 1288–1298), there is **no** `advance_after_elicitation_completion` — the per-chat queue may **not** advance, so the next session may never become the active Telegram token for interactive surfaces.
- **Plain-text routing:** `telegram_message_handler` routes **non-command** plain text to `handle_elicitation_other_followup_plain_message` only (`telegram_bot.rs` lines 63–77). There is **no** generic “route to active session” for TextInput elicitation without `/answer-text` or pending Other — `active_elicitation_session_for_chat` is exposed on the harness (`telegram_session_control.rs` lines 766–774) but **not** wired in `telegram_bot` for arbitrary plain text.
- **`/answer-text` / `/answer-multi`:** `handle_answer_text_command` / `handle_answer_multi_command` do **not** advance the elicitation queue after success (`telegram_session_control.rs` lines 1301–1353). Under PRD “single visible question,” completing elicitation via these commands should likely **rotate** the queue like `handle_elicitation_select`.
- **Document-review concurrency:**
  - **Outbound:** Non-primary sessions defer inline keyboards (`telegram_notifier.rs` lines 549–575) — aligned with queuing.
  - **Inbound:** No `elicitation_callback_permitted` on `doc:*` (`telegram_bot.rs` lines 283–300).
  - **Completion:** `handle_document_review_action` does **not** call `advance_after_elicitation_completion` (`telegram_session_control.rs` lines 1129–1156), unlike `handle_elicitation_select` (lines 1183–1193). So approving/rejecting a document does **not** promote the next queued session.

---

## Recommendations before merge

1. **Unify queue advancement:** After any successful elicitation completion that should release the chat token (at minimum: document review terminal actions, Other follow-up, `/answer-text`, `/answer-multi` where applicable), call the same `advance_after_elicitation_completion` pattern as `handle_elicitation_select` (`telegram_session_control.rs` ~1183–1193), with clear logging of `next_active_session_id`.
2. **Document-review inbound policy:** Either gate `doc:*` with `elicitation_callback_permitted` (mirror `eli:s:` / `eli:o:`) or document why stale `doc:` buttons must remain usable; if gated, use the same user-facing alert as select (`telegram_bot.rs` ~320–325).
3. **Replace `unwrap()` on production mutexes** with `map_err` / poison handling or a dedicated lock helper to avoid daemon-wide panic on rare poison cases.
4. **Bound or monitor queue length** for `ActiveElicitationCoordinator` (configurable max, warn log + operator message when exceeded).
5. **Validate callback_data** at send time: if `clarification_select_keyboard` skips buttons (`telegram_notifier.rs` ~819–827), consider failing closed or sending a warning Telegram message so the user is not answering a truncated question.
6. **Run and record** `./verify` or `./test -p tddy-daemon` and confirm `active_elicitation` tests pass without accidental marker-induced failures (see `.red-test-output.txt` if still present).

---

## References (file:line)

| Topic | Location |
|--------|----------|
| Shared coordinator construction | `main.rs` ~177–186, ~216–223 |
| Register + defer primary keyboard | `telegram_notifier.rs` ~512–575 |
| Queue advance / drain | `active_elicitation.rs` ~65–111 |
| Select advances queue | `telegram_session_control.rs` ~1159–1209 |
| Document review (no advance) | `telegram_session_control.rs` ~1129–1156 |
| Other follow-up (no advance) | `telegram_session_control.rs` ~1246–1298 |
| Elicitation callback gates | `telegram_bot.rs` ~303–371 |
| Document callbacks (no active gate) | `telegram_bot.rs` ~283–300 |
| `callback_data` length handling | `telegram_notifier.rs` ~801–854 |
| `pending_elicitation_other` field | `telegram_session_control.rs` ~626–629 |

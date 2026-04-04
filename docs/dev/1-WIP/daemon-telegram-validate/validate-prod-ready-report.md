# Validate production readiness — Telegram integration (`tddy-daemon`)

**Scope:** `packages/tddy-daemon/src/telegram_notifier.rs`, `packages/tddy-daemon/src/config.rs` (`TelegramConfig`), `packages/tddy-daemon/src/main.rs` (integration surface), cross-check with `docs/dev/1-WIP/daemon-telegram-validate/evaluation-report.md`.

**Sources reviewed:** Current workspace sources as of this assessment (not speculative APIs).

---

## Executive summary

The Telegram module is implemented as a **library** with async send helpers, config structs (`telegram:` YAML, `deny_unknown_fields`), and tests, but it is **not integrated** into the running daemon: `main.rs` never constructs a `Bot`, `TelegramSessionWatcher`, or calls `on_metadata_tick`, so **no production notifications occur**. Within the module itself, send failures are returned as `anyhow::Error` (no `panic!`/`unwrap` on the send path); the main production risk for stability is therefore **how a future caller handles `Result`** (must not `unwrap`/`expect` on the hot path). Security is partially addressed: `mask_bot_token_for_logs` exists and avoids substring leakage, but it is **not used** at config load or startup—there is also no current log line that prints `bot_token`, while `TelegramConfig` **derives `Debug`**, which would print the raw token if someone logs `{:?}` on config. Logging uses a dedicated target `tddy_daemon::telegram` and avoids secrets in the teloxide helper (logs `chat_id`, message length). `marker_json` is a **debug** trace (despite the name, messages are plain key=value strings, not JSON objects in the source), invoked from many call sites and may be noisy if debug logging is enabled broadly. Configuration allows `enabled: false` with a present block; **`enabled: true` with empty `chat_ids`** updates state but sends nothing, with only indirect visibility (`chat_targets=0` in an info line on transition). **Gaps vs the evaluation report:** all major findings there remain valid—especially missing `main.rs` / session-polling wiring and the empty-`chat_ids` visibility gap.

---

## Checklist (severity)

| Severity | Area | Finding | Evidence |
|----------|------|---------|----------|
| **Blocker** | Product / wiring | Telegram is not connected to daemon lifecycle; no runtime sends. | `main.rs` loads config and runs `server::run_server` only; no `telegram_notifier` imports or tasks. |
| **Blocker** | PRD / E2E | End-to-end “daemon observes session metadata and notifies” is not satisfied until integration exists. | Aligns with `evaluation-report.md` Validity Assessment. |
| **High** | Error handling | `on_metadata_tick` uses `sender.send_message(...).await?` in a loop; first successful send can occur before a later failure—**partial delivery** with error return. | `telegram_notifier.rs` transition branch: loop over `tg.chat_ids`. |
| **High** | Integration contract | Future integration must treat `anyhow::Result` from `on_metadata_tick` as non-fatal (log + continue); otherwise a single Telegram API error could tear down a larger task. | Library returns `Err`; no integration code exists yet to define behavior. |
| **Medium** | Configuration | `enabled: true` + empty `chat_ids`: state advances, zero sends, easy to misconfigure silently. | Same as `evaluation-report.md` § Issues; `on_metadata_tick` logs `chat_targets={}` only on transition. |
| **Medium** | Configuration | YAML requires `bot_token` when a `telegram:` block is present (`pub bot_token: String`); invalid minimal YAML fails at parse—acceptable strictness, but operators need a clear example. | `config.rs` `TelegramConfig`. |
| **Medium** | Security | `#[derive(Debug)]` on `TelegramConfig` / `DaemonConfig` risks accidental secret logging if `log::debug!("{:?}", config)`-style debugging is added. | `config.rs`; no such log found in current daemon sources for Telegram. |
| **Low** | Logging | Full `session_id` appears in log lines (info/debug); may be sensitive in shared log sinks. | e.g. `on_metadata_tick` transition and baseline logs. |
| **Low** | Logging | `marker_json` name suggests JSON; implementation logs a string; frequent **debug** calls when Telegram code paths run. | `telegram_notifier.rs` `marker_json`. |
| **Low** | Performance | `TelegramSessionWatcher.last_status` grows per `session_id` with no eviction shown in-module. | `HashMap<String, String>` only; long-lived daemon may need a policy when integrated. |
| **Info** | Dependencies | Teloxide adds lockfile surface; supply-chain/compile cost noted in evaluation. | `evaluation-report.md`. |
| **Info** | Repository | Untracked `.telegram-red-test-output.txt` should not ship with commits. | `evaluation-report.md`. |

---

## Gaps vs PRD (per evaluation-report framing)

The evaluation report states that the change set **partially** satisfies a PRD-style requirement: library behavior and tests are in place, but **“detect status changes from real session metadata and notify from the running daemon”** is **not** met until:

1. **`main.rs` (or equivalent startup)** wires a `TelegramSessionWatcher` and a real `TelegramSender` implementation backed by teloxide (`send_telegram_via_teloxide` + `Bot` constructed with configured token—pattern not present in tree).
2. A **polling or notification loop** invokes `on_metadata_tick` with live `session_id`, `status`, and `is_active` from the same sources the daemon already uses for session truth.

Until then, **acceptance criteria that require a live status change through the daemon remain open**.

---

## Actionable recommendations

1. **Integrate in `main.rs` / server:** After `tokio` runtime creation, spawn a task (or hook into an existing periodic session scan) that holds `TelegramSessionWatcher`, builds `Bot::new(tg.bot_token)` (or equivalent), implements `TelegramSender` for production, and calls `on_metadata_tick` on each metadata observation. **Confirm** error handling: log errors at `error!` or `warn!` and continue the daemon.
2. **Empty `chat_ids`:** When `tg.enabled && tg.chat_ids.is_empty()`, log a **`warn!`** once at startup (or first tick), as suggested in `evaluation-report.md`.
3. **Avoid secret leakage:** Prefer logging `mask_bot_token_for_logs(&tg.bot_token)` if any diagnostic logs Telegram config; avoid `{:?}` on structs that contain `bot_token`, or implement a custom `Debug` for `TelegramConfig` that redacts.
4. **Partial multi-chat failure:** Document or adjust behavior: e.g. send to all chats and aggregate errors, or use `try_join!`-style semantics—current code stops at first `Err`.
5. **Memory:** When integrating, define whether `last_status` entries are pruned when sessions are deleted or TTL’d (coordinate with session lifecycle APIs).
6. **Trim or gate `marker_json`:** Before production, reduce debug noise or restrict to a feature flag / higher trace level if still needed for support.
7. **Hygiene:** Remove or gitignore `.telegram-red-test-output.txt` before merge.

---

## Negative findings (what is already in good shape)

- **No panic on API failure in-module:** `send_telegram_via_teloxide` maps errors to `anyhow` and returns `Result`; `on_metadata_tick` propagates sender errors without unwinding.
- **Dedicated log target:** `target: "tddy_daemon::telegram"` allows filtering.
- **Teloxide send path:** Logs `text_len` and `chat_id`, not message body at info (reduces accidental PII in default verbosity).
- **YAML strictness:** `deny_unknown_fields` on `DaemonConfig` and `TelegramConfig` catches typos.
- **Tests:** Unit and integration tests cover masking, labels, transitions, disabled config, and terminal non-spam (per evaluation report).

---

## Conclusion

**Production readiness:** **Not ready** for end-user Telegram notifications until runtime integration and operational hardening (empty targets, error policy, optional memory bounds) are completed. Library-level error handling is compatible with a non-panicking daemon **if** the future integration layer handles `Result` correctly.

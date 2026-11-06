# 2026-11-03 — Cursor's hand-built launch helpers Telegram still calls

**Category:** Deferred from `#agent-tools` 7/12 (#796); narrowed by `#agent-tools` 17/19 (#823,
`feature/agent-tools/cli-dispatch-collapse`)
**Source:** `TODO(cli-dispatch-collapse)` at `packages/tddy-agent-launch/src/chat.rs`
(`install_cursor_hooks_in_worktree`, `mint_cursor_chat_id`) and
`packages/tddy-cli-sessions/src/cli_session_manager/launch.rs` (`CliSessionManager::start`,
`start_cursor`)

## What is there

`#agent-tools 17/19` deletes the claude and cursor dispatch and every hand-built daemon launch of
them: `prepare_cursor_cli_launch`, `spawn_cursor_cli_process`, the per-agent bodies of
`spawn_cursor_cli_session_inner` / `…_reporting`, `resume_cursor_cli`, `launch_jailed_cursor`.
What this record listed earlier as having **no caller** is stale for four helpers: Telegram's own
launcher calls them.

| Code | Caller |
|---|---|
| `install_cursor_hooks_in_worktree` (`tddy-agent-launch/src/chat.rs`, re-exported through `cursor_cli_spawn`) | `tddy-telegram-control/src/telegram_session_control/cli_spawns.rs:350` |
| `mint_cursor_chat_id` (`chat.rs`) — also the function behind the `cursor-create-chat` adapter | `cli_spawns.rs:361` |
| `CliSessionManager::start` (`tddy-cli-sessions/src/cli_session_manager/launch.rs`) | `cli_spawns.rs:132` |
| `CliSessionManager::start_cursor` (`launch.rs`) | `cli_spawns.rs:370` |

## Why it was deferred

Telegram's launcher is outside `#agent-tools 17/19`'s boundary; routing it through the one spec
launch is `#agent-tools 18/19` (`feature/agent-tools/telegram-spec-launch`). Deleting the helpers
first would break Telegram's `/start_claude` and `/start_cursor`.

## What would close it

`#agent-tools 18/19` routes Telegram's starts through the spec launch, then deletes the four
helpers (keeping `mint_cursor_chat_id` only as the adapter's body if the adapter still wraps it)
and the argv builders only they use (`build_claude_argv`, `with_claude_launch_flags`,
`build_cursor_argv`).

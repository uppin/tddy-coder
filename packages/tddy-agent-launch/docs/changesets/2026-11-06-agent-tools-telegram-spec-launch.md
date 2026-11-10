# 2026-11-06 — An os-user start entry; the Cursor hand-built helpers are gone

**Type:** Feature, with planned deletions

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)).

- `operator_start.rs`: `LaunchSessions::start_session_for_os_user(os_user, req, progress)` — `start_session_core` without the token-to-user step; refuses a session token or a peer daemon in the request. `start_here` and `refuse_a_requested_role` became `pub(crate)`.
- Deleted (planned): `chat::install_cursor_hooks_in_worktree` and `chat::mint_cursor_chat_id` — Telegram was their last caller. `cursor_cli_spawn` keeps only the notify environment.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-telegram-spec-launch.md).

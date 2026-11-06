# 2026-11-06 — The generic session start has no deadline of its own

**Category:** Parent finding, recorded by `#agent-tools` 18/19 (`feature/agent-tools/telegram-spec-launch`) for the sweep node (n20)
**Source:** `packages/tddy-agent-launch/src/operator_start.rs` (`LaunchSessions::start_session_for_os_user`), `packages/tddy-agent-launch/src/svc_start_session_core.rs` (`LaunchSessions::start_session_core`)

## What is there

Both entries run the generic start under a fresh `CancellationToken::new()` and no overall
deadline. Each step inside it carries its own (`spawn_worker_request_timeout` for the worktree, the
`cursor-create-chat` mint deadline from `#agent-tools 14/19`), but nothing bounds the start as a
whole: a step without a deadline, or a sequence of slow ones, keeps the caller waiting.

Telegram, the first satellite caller, bounds its call itself: `START_DEADLINE` (600 s) in
`packages/tddy-telegram-control/src/telegram_session_control/cli_spawns.rs`. Every other caller of
`start_session_core` (the web RPC, the in-memory peer RPC) has no such bound.

Reported, not fixed: `start_session_core` is parent code (`#agent-tools 17/19` and earlier), and
the start is shared by every caller.

## What would close it

One deadline owned by the start itself (a `DaemonConfig` knob next to
`spawn_worker_request_timeout`, cancelling the start's token when it passes), applied in
`start_here` so both entries inherit it. Then drop Telegram's `START_DEADLINE`, or keep it only as
a chat-side message.

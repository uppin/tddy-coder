# 2026-11-04 — A Telegram-initiated delete leaves an agent session's worktree on disk

**Category:** Regression (accepted, moved)
**Owner:** `#agent-tools` n19 `agent-config-removal` ([#825](https://github.com/uppin/tddy-coder/pull/825)) (`feature/agent-tools/agent-config-removal`) — moved out of #801 by the developer on 2026-11-04, to be planned on top of
`feature/agent-tools/agent-registry`. Code marker: `TODO(agent-config-removal)`.
**Source:** `#agent-tools` 12/19 (#801), `packages/tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs`
(`handle_delete_session`)

## What is lost

`delete_session_directory` decides which sessions own their worktree from the agent's spec
(`worktree_removal_applies_to(agents, session_type)`: every `<id>-cli` session except an agent that
declares `keep_worktree_on_delete`, plus `workspace`). The Telegram controller holds no coding-agent
registry and passes `None`, so a `/delete` from Telegram removes the session directory but **leaves an
agent session's git worktree on disk** — before #801 a Telegram delete of a `claude-cli`,
`opencode-cli`, `muse-cli` or `warp-cli` session removed it. `workspace` sessions are unaffected, and
`ConnectionService::DeleteSession` (web, RPC) still removes the worktree.

## What would close it

Hand `TelegramSessionControlHarness` the host's `CodingAgentRegistry` (as `DaemonSessionHost` holds
it) and pass it to `delete_session_directory`, with a Telegram acceptance test that deleting an agent
session removes its worktree. Part of n19 `agent-config-removal` ([#825](https://github.com/uppin/tddy-coder/pull/825))'s scope (see
[2026-11-04-removed-agent-config-readers-remain.md](2026-11-04-removed-agent-config-readers-remain.md)).

# 2026-10-25 — A cursor conversation's `.cursor/mcp.json` outlives the conversation

**Category:** Deferred from `#coagents` 9/9 (#776, sweep)
**Source:** `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`write_cursor_project_mcp_config`); `packages/tddy-session-agents/src/coding_agent.rs` (`AgentRoster::conversation_closed`)

A headless `cursor` conversation is handed `tddy-tools --mcp` through `<worktree>/.cursor/mcp.json`
(owner-only, listed in the repository's `info/exclude`), because cursor reads only a project or a
home MCP config. `AgentRoster::conversation_closed` removes a conversation's approval route and its
`headless/<conversation>/` directory, but it does not know the conversation's worktree, so cursor's
file — which carries the agent's tool credential — stays after the close. The start-up sweep does
not reach it either: it only walks `<tddy_data_dir>/headless/`.

Two consequences:

- the credential stays on disk in the worktree (still owner-only and git-ignored) until the worktree
  itself is removed;
- a later headless `cursor` launch on the **same** worktree is refused, because an existing
  `.cursor/mcp.json` is never overwritten (it could be the repository's own, and tracked). A
  clone-placed conversation gets a worktree of its own, so this bites a retry under the same
  conversation id: a launch whose open fails after the plan leaves the file, and the retry is refused.

Deferred because closing it needs the close path to know the conversation's worktree (or the
launcher to record the file it wrote under `headless/<conversation>/` so the close and the sweep can
find it), which is a new piece of state the sweep node did not plan.

## What would close it

Record the path of cursor's file in the conversation's `headless/<conversation>/` directory (beside
`daemon-instance`) at plan time; `conversation_closed` and `sweep_headless_dirs` remove the recorded
file when it is still the one the launcher wrote. A test opens and closes a headless cursor
conversation and asserts the worktree has no `.cursor/mcp.json` afterwards, and that a second launch
on the same worktree succeeds.

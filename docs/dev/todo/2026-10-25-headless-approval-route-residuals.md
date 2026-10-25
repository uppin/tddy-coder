# 2026-10-25 — A headless conversation's approval route and `mcp.json` outlive a failed open

**Category:** Deferred from `#coagents` 7/8 (#726, `headless-driver`) — residual
**Source:** `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`headless_plan`, `bind_approvals`, `write_mcp_config`); `packages/tddy-session-agents/src/coding_agent.rs` (`AgentRoster::conversation_closed`)

`headless_plan` registers the conversation's launch in `ApprovalRoutes` (`register_launch`) and then
writes `<tddy_data_dir>/headless/<conversation>/mcp.json`, which carries the agent's minted tool
credential. Both are removed only by `AgentRoster::conversation_closed`, which runs when the
conversation is cancelled or its session closes.

- **Inert route entry.** When the open fails after the plan (the session never starts), the route's
  `launch` half stays registered with no `permissions` half until the conversation id is closed. It
  answers nothing — `owner_of` refuses a route without permissions — but it holds the credential in
  memory.
- **`mcp.json` lifetime.** The file stays on disk for the whole conversation and after a failed open,
  until close. A daemon that crashes leaves it behind; nothing sweeps `headless/` at start.

## What would close it

Tie both to the open: a guard returned with the `HeadlessCommand` that removes the route and the
directory unless the session start succeeds, and a start-up sweep of `<tddy_data_dir>/headless/`
for conversations the daemon no longer holds.

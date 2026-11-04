# 2026-11-04 — Two waits on the ACP launch path have no deadline

**Category:** Deferred from `acp-agents` (#800, `#agent-tools 11/12`) to the `#agent-tools` sweep node

**Source:** `docs/dev/changesets/2026-11-04-agent-tools-acp-agents.md`, § Deferred

## What remains

Neither wait is new in #800. Both sit in parent code that #800's spec-planned launches now reach:

- `packages/tddy-agent-backend/src/backend/acp_driver.rs` `AcpDriver::open_session` awaits the
  worker's `session/new` (or `session/load`) reply without a timeout. An ACP agent that starts but
  never answers `session/new` leaves the coding-subagent launch waiting forever.
- `packages/tddy-supervisor/src/client.rs` `SupervisorClient::call` → `transport.call_unary` has no
  deadline. `coding_agent_relay.rs` `spawn_relayed` reaches it for every relayed coding-agent spawn,
  so a supervisor that accepts the connection and never replies hangs the launch.

The stack rule is that nothing may hang.

## Why it is deferred

The developer deferred both to the `#agent-tools` sweep node on 2026-11-04. Each needs its own
deadline value and a refusal message, and neither is part of #800's responsibility (planning ACP
launches from specs).

## What would close it

A bounded wait at each site, refusing with an error that names the agent (or the supervisor method)
and the deadline, plus a test against a peer that never replies.

# 2026-11-04 — The supervisor wait on the ACP launch path has no deadline

**Category:** Deferred from `acp-agents` (#800, `#agent-tools 11/12`) to the `#agent-tools` sweep node

**Source:** `docs/dev/changesets/2026-11-04-agent-tools-acp-agents.md`, § Deferred

**Status:** Open — partially fixed (2026-11-05, #822 `#agent-tools 16/19`): `AcpDriver::open_session`
now has a deadline (`SESSION_OPEN_DEADLINE`, 120 s, `open_session_within`) and refuses naming the
agent; pinned by `tddy-agent-backend/tests/spec_backend_integration.rs`. The supervisor wait remains.

## What remains

- `packages/tddy-supervisor/src/client.rs` `SupervisorClient::call` → `transport.call_unary` has no
  deadline. `coding_agent_relay.rs` `spawn_relayed` reaches it for every relayed coding-agent spawn,
  so a supervisor that accepts the connection and never replies hangs the launch.

The stack rule is that nothing may hang. The wait is not new in #800; it sits in parent code that
#800's spec-planned launches now reach.

## Why it is deferred

The developer deferred it to the `#agent-tools` sweep node on 2026-11-04. It needs its own deadline
value and a refusal message, and it is not part of #800's or #822's responsibility.

## What would close it

A bounded wait in `SupervisorClient::call`, refusing with an error that names the supervisor method
and the deadline, plus a test against a supervisor peer that never replies.

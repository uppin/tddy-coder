# 2026-11-04 — The hand-built ACP launch path stays for launchers built without coding agents

**Category:** Deferred from `acp-agents` (#800, `#agent-tools 11/12`)

**Source:** `docs/dev/changesets/2026-11-04-agent-tools-acp-agents.md`, § Deferred

## What remains

`DaemonCodingAgentLauncher::{new, with_identity}` build a launcher with no `HostCodingAgents`. For
those launchers, `coding_agent_launcher.rs` still plans ACP launches by hand (`harness_command`,
`apply_route`, the ACP constants, `claude_agent_acp_command`, `opencode_route_config`,
`opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`, `ACP_HARNESSES`,
`withdrawal_session_meta`). It is marked `TODO(acp-agents)` at the branch that reaches it.

## Why it stays

The production constructor, `for_daemon`, always installs coding agents (or refuses on a registry
load error), so the hand-built branch is reached only by tests. Deleting it now would mean migrating
the `tddy-daemon-rpc` acceptance suites (about eight files) to `with_coding_agents(...)`, which is
outside #800's scope. The developer chose to keep it for this node and not to add a
`FailedPrecondition` refusal there.

## Sweep work

The `#agent-tools` sweep node migrates the session-lifecycle and `tddy-daemon-rpc` fixtures to
`with_coding_agents`, then deletes the hand-built path and the items listed above.

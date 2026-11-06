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

## Narrowed by `#agent-tools` 19/19 (`agent-config-removal`)

`DaemonCodingAgentLauncher` now holds its coding agents unconditionally (an empty registry when none
are installed), and `coding_agent_launcher.rs` plans nothing by hand: `harness_command`,
`apply_route`'s hand-built branch, `claude_agent_acp_command`, the ACP/binary constants and the
hand-built `mcp_servers_for` / `session_meta_for` answers are deleted, their tests moved onto stub
binaries behind the spec path.

**Still open (a different crate, so not this node's):** `tddy-coding-subagent` keeps
`withdrawal_session_meta`, `opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`
and `ACP_HARNESSES`. The first is the default body of `CodingAgentLauncher::session_meta_for`
(`launcher.rs`) and `ACP_HARNESSES` backs `transport_of`; deleting them changes that trait's default
and its own unit tests, which the sweep node (n20) owns.

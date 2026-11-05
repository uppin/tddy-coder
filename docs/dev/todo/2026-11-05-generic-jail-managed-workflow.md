# 2026-11-05 — The generic jailed CLI launch wires no managed workflow

**Category:** Gap
**Owner:** `#agent-tools` n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)), which deletes the bespoke jailed claude start and so needs this. Code marker: `TODO(cli-spec-features)` at `packages/tddy-agent-launch/src/cli_agent_steps.rs` (`refuse_a_managed_codebase_in_a_jail`).
**Source:** `#agent-tools` 14/19 (#820)

## What is there

`launch_jailed_cli_agent` (the generic jailed start and resume) now binds the conversation, the
permission mode and the session id for claude's and cursor's specs, but it passes `managed: None` and
renders no hooks, MCP, tool-list or role slot (`ClaudeJail` does, for the bespoke claude jail). A
sandboxed start of a spec declaring `managed_workflow` over a managed codebase is therefore refused
`FAILED_PRECONDITION` rather than started unmanaged.

## What remains

Carry `ManagedWorkflow`, the MCP server (`mcp_server_env`), the allow/deny lists and the permission
prompt tool through `CliJail`, keyed on the spec's slots and capabilities, then drop the refusal.

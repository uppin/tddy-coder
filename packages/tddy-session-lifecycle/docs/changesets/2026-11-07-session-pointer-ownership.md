# 2026-11-07 — A headless Codex conversation reaches its tool server through a pointer

**Type:** Feature

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- `coding_agent_launcher.rs`: for an inline MCP slot, `write_conversation_pointer` writes `<conversation dir>/.tddy/session.json` (owner-only, atomic, no socket, the server env in `remote`, owned by the daemon) and the server gets `cwd` = the conversation directory and an empty `env`. `forwards_server_env` is deleted, so the `HeadlessCommand` env carries no `TDDY_*`.
- Tests: `tests/headless_subagents_from_spec_acceptance.rs` (`a_headless_codex_conversation_carries_no_tddy_env_and_resolves_through_its_pointer`; the codex overrides render `cwd` and no `env_vars`), `coding_agent_launcher/headless_tests.rs`.

## Restructuring (engine-driven, `tddy-tools restructure move_impl_members` / `move_item`)

`coding_agent_launcher.rs` **990 → 424** production lines (to the first `#[cfg(test)]`):

- `coding_agent_launcher/headless.rs` (399): the headless plan — `headless_plan`, `headless_plan_from_spec`, `write_conversation_pointer`, `headless_provider_env`, `record_written_files`, `claim_conversation_files`, `bind_approvals`, `refuse_unless_daemon_user`, `ClaimedFiles`, `headless_route_env`, `registers_mcp_inline`, and `sweep_headless_files_left_by_this_daemon` (re-exported from `coding_agent_launcher` for `tddy-daemon`).
- `coding_agent_launcher/acp.rs` (256): the ACP plan — `acp_plan_from_spec`, `acp_mcp_servers_from_spec`, `acp_session_meta_from_spec`, `acp_spec_for`, `confine`, `confinable_acp_agents`, `withdrawn_tools`, `acp_route`, `tool_server_command`, `acp_agents_for`, `opencode_route_config`, `command_before_user_switch`.

## Code issues

- **Closed** `oversized-file-coding-agent-launcher` — 1294 at its worst (#819), 990 on `origin/master`, **424** after the split. Record deleted.
- **Closed** `complexity-coding-agent-launcher-headless-plan` — 91 at detection; `headless_plan` **34**, and the body it delegates to, `headless_plan_from_spec`, **56** (62 on `origin/master`; `registers_mcp_inline` extracted). Record deleted.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-session-pointer-ownership.md).

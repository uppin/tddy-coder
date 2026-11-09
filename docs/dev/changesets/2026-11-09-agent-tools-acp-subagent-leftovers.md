# 2026-11-09 — `tddy-coding-subagent` names no agent; an ACP session runs with its agent's declared hooks

**Type:** Refactor

`#agent-tools` 27/32 ([#835](https://github.com/uppin/tddy-coder/pull/835)). Packages:
`tddy-coding-subagent`, `tddy-coding-agent-specs`, `tddy-session-agents`, `tddy-session-lifecycle`,
`tddy-daemon-rpc`, `tddy-session-split` (test fixture only).

## What changed

- **Agent data lives in the specs.** A subagent's transport, permission policy, catalog offer and
  managed-session confinement are read from the host's `CodingAgentRegistry`:
  `transport_of_spec`, `permission_policy_of_spec`, `transport_and_policy_wire_of_spec`,
  `coding_agent_defs_from(registry, profiles, allowed)`, `refuse_unless_registry_confinable(registry,
  harness, managed)`, `confinable_acp_agents(registry)`. A headless spec declares who answers its
  permission requests in the new `coding_agent.headless.permission_policy` (`asks_caller` |
  `{ fixed: <text> }`, `HeadlessPermissionPolicy`), declared in `claude`, `cursor`, `codex` and
  `oz.yaml`; a headless agent declaring none is not offered. A headless agent stays refused in a
  managed session: its own-tools flag is not an ACP withdrawal.
- **`tddy-coding-subagent` names no agent** in production code, pinned by `tests/no_agent_names.rs`.
  `CodingAgentLauncher::session_meta_for` is a required method (no default naming agents).
- **Hooks adapters are honoured by the daemon launcher.** `CodingAgentLauncher::launch_agent` →
  `LaunchedAgent { transport, hooks }` (default: `launch_transport` + `no_launch_hooks()`);
  `DaemonCodingAgentLauncher` prepares the spec's `acp.hooks_adapter` on the command before the user
  switch, so a daemon-launched `codex-acp` gets `codex-acp-login`'s one `codex login` retry. The
  adapter is handed no route: the plan renders it (`-c` overrides on `route.shape: toml_args`, the
  inline `OPENCODE_CONFIG_CONTENT` on `json_config_file`), so no override is doubled and no route
  file is written. `CodingAgentSession::start` / `start_over` take the hooks, bracket the open with
  them (handed the open's `cwd`) and hold them for the session's lifetime.
- **The registry is threaded through.** `AgentRoster.coding_agents`, `coding_agent_defs_for(…,
  coding_agents)`, `refuse_unconfinable_harness(…, coding_agents)`, `start_coding_agent_session(…,
  coding_agents)`, daemon-rpc `coding_agent_rows` / `coding_agent_info`, and the roster snapshot
  (`SnapshotSources::coding_agents`; `DaemonSessionHost::with_coding_agents` rebuilds the roster store
  over the host's agents).

## Decisions (developer, 2026-11-07)

- **D1** — the OpenCode route stays inline (`OPENCODE_CONFIG_CONTENT`): a route file the daemon writes
  cannot be read by an agent started as the session's OS user. A route file owned by the session user
  is a follow-up node; this PR adds only the hooks plumbing it will use.
- **D2** — full scope: a new spec field, transport and policy from the spec, catalog and attach-time
  confinement from the registry, `tddy-session-agents` and `tddy-daemon-rpc` included.
- **D3** — `session_meta_for` is a required method.
- **D4** — `launch_agent` prepares every declared `acp.hooks_adapter`. With D1, each adapter is handed
  no route, so `opencode-route-file` writes nothing and `codex-acp-login` adds no second `-c`.
- At green: the open bracket is handed the open's `cwd` (the worktree); headless `claude` stays
  refused in a managed session; the name-keyed wrappers stay (their deletion was not approved).

## Removed (approved, 2026-11-07)

`confinement_for`, `refuse_unless_confinable`, `withdrawal_session_meta`, `opencode_withdrawal_config`,
`CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS` and their inline tests; `CLAUDE_ACP`, `CODEX_ACP`,
`OPENCODE`, `ACP_HARNESSES`, `HEADLESS_HARNESSES`. `OPENCODE_CONFIG_CONTENT` stays (the route stays
inline). The name-keyed `transport_of`, `permission_policy_of`, `transport_and_policy_wire` and
`coding_agent_defs` stay as built-in-registry wrappers for a caller with no host.

## Backlog

- Resolved and deleted: `2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md`.
- Kept: `2026-11-04-opencode-acp-route-file-has-no-session-owner.md` — a route file the session user
  owns is a follow-up node (D1).
- Added: `2026-11-09-daemon-codex-acp-login-runs-as-the-daemon-user-in-the-worktree.md` — the login
  retry runs as the daemon's user and writes its authorize URL into the worktree.
- Added: `2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md` — the
  two engine defects the `session.rs` split hit (below).

## Restructuring (engine-driven, `tddy-tools restructure move_item`)

`tddy-coding-subagent/src/session.rs` **729 → 756 → 445** production lines (base → green → split,
counted to the first `#[cfg(test)]`), under the 500 budget:

- `session/cancel.rs` (84): `CANCEL_SETTLE_DEADLINE`, `CancelHandle` and its `TurnCancelling` impl,
  `drive_cancelled_turn_to_its_end` (`CancelHandle` re-exported at the crate root as before).
- `session/subagent_session.rs` (134): the `SubagentSession` impl of `CodingAgentSession`.
- `session/message_shape.rs` (74): `ToolCallRecord`, `collect_tool_calls`, `MessageShape`.
- `session/turn_rules.rs` (55): `new_prompt`, `refused_while_pending`, `answer_delivered_while_held`,
  `stop_reason_of`, `refuse_loop_only`.

Post-move fixes by hand: `ToolCallRecord` widened to `pub(super)` (the engine widened `MessageShape`
and `collect_tool_calls` but not the record type `collect_tool_calls` returns), the copied imports
removed with `cargo fix` (the engine's tidy did not run after its compile gate failed on that
visibility), and one intra-doc link re-pointed (`super::CodingAgentSession`). The first plan's
seven moves stopped after five ("anchor invalidated … inside text removed by an earlier
operation"); the last two were re-anchored and applied as a second plan. `restructure verify
--against HEAD`: 506223 statements before and after, only the re-pointed doc line differs.

## Code issues

- `tddy-coding-subagent` `oversized-file-session.md` — **closed**: 731 at its last row, 756 at this
  PR's green, 445 after the split. Deleted.
- `tddy-coding-subagent` `complexity-session-take-turn.md` → `complexity-subagent-session-take-turn.md`
  — moved with the `SubagentSession` impl, unchanged (78 raw lines, depth 5).
- `tddy-coding-agent-specs` `oversized-file-spec.md` — **regressed** 981 → 1031
  (`HeadlessPermissionPolicy`). Not split: #836 and #857 above this PR modify `spec.rs`; left for the
  `#agent-tools` sweep node.

# 2026-11-05 — The generic CLI spec launch serves claude's and cursor's needs

**Type:** Feature

`#agent-tools` 14/19 ([#820](https://github.com/uppin/tddy-coder/pull/820)). Packages:
`tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-agent-launch`, `tddy-session-lifecycle`.

- `tddy-coding-agent-specs`: `Capability::ManagedWorkflow` (`managed_workflow`, declared by
  `claude.yaml` and `cursor.yaml`); `slots::conversation::ConversationSlot`
  (`coding_agent.conversation: { kind: daemon_session_id | minted | worktree | resume_token }`,
  `mint_deadline_secs` defaulting to 30 s), `CodingAgentSpec::conversation()` (derived when absent),
  and two load refusals (an unknown conversation adapter; `resume_token` without a `resume` slot).
- `tddy-agent-backend`: `adapters::conversation_id::{ConversationIdAdapter, CursorCreateChat}` and
  `LaunchAdapters::conversation_id`; `cursor-create-chat` is killed at its deadline.
  `chat::mint_cursor_chat_id` delegates to it.
- `tddy-agent-launch`: the generic start and resume (`opencode_cli_spawn`, the generic jail) gain the
  spec-keyed steps of `cli_agent_steps` — hooks, MCP / tool-list / system-prompt slots, the managed
  workflow, operator notify, the project-home role, ssh slot files on the target, participant
  metadata and the conversation. `refuse_an_unsupported_cli_session_start` reads the spec
  (`managed_workflow`); `refuse_a_type_main_cannot_run` reads `project_home_main` (it was the literal
  `claude-cli`). `HarnessLaunch` binds `session_id`, `chat_id`, `permission_mode`,
  `dangerously_skip_permissions` and `node_launcher`; `claude_template_params` and
  `cursor_template_params` are deleted and every launch binds through `harness_template_params`.
  The generic host PTY runs under `env -u` for each inherited `TDDY_*`, and a generic tmux pane
  unsets every inherited `TDDY_*` but the launch's own before it execs the agent
  (`in_a_pane_without_inherited_tddy`).
- `tddy-session-lifecycle` (tests): `cli_spec_features_acceptance` (copies of `claude.yaml` /
  `cursor.yaml` under another id: tmux, project-home main, ssh target, minted chat, the two
  deadlines, jailed claude copy, no daemon `TDDY_*` on an ssh start) and `yaml_only_agent_acceptance`
  (managed workflow with hooks, the managed refusal, both conversation kinds, no `TDDY_*` on the
  host, in tmux and in a jail).

Backlog: none resolved. Recorded and kept: `2026-11-05-generic-jail-managed-workflow` (owned by n17,
which must carry the managed workflow in the generic jailed launch before it deletes the bespoke
jailed claude start) and `2026-11-05-cli-spec-features-leftovers` (bespoke tmux markers, managed/role
environment without an `mcp` slot, renderer env, a minted chat recorded late). The references
`2026-11-04-claude-and-cursor-launch-by-agent-id`, `2026-11-02-claude-cli-hand-built-launch-removal`
and `2026-11-03-cursor-hand-built-launch-paths` stay with n17.

Code issues (measured on `origin/feature/agent-tools/route-support` `3c29698cd` and HEAD; restructuring
deferred to the `#agent-tools` sweep node by the developer):

- `tddy-agent-launch`, new: `oversized-file-opencode-cli-spawn` (477 → 632 production lines),
  `oversized-file-spec-launch` (451 → 525), `oversized-file-cli-agent-steps` (0 → 571),
  `complexity-cli-agent-steps-wire-session-agent` (80 lines), `…-resumed-session-wiring` (72).
- `tddy-agent-launch`, regressed: `start_cli_harness_session` 200 → 303, `resume_cli_harness_session`
  74 → 97, `start_cli_harness_from_request` 60 → 89, `start_sandboxed_cli_harness_session` 223 → 239,
  `resume_sandboxed_cli_harness` 73 → 74; `claude_cli_spawn_steps.rs` 537 → 558, `cursor_cli_spawn.rs`
  682 → 684, `svc_start_sandboxed_cursor_cli_session.rs` 549 → 552.
- `tddy-coding-agent-specs`: `spec.rs` 736 → 769 (regressed); `slots::render` 69, unchanged.

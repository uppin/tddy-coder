# 2026-11-05 — The generic CLI launch serves claude's and cursor's needs

**Type:** Feature

`#agent-tools` 14/19 ([#820](https://github.com/uppin/tddy-coder/pull/820)).

- `cli_agent_steps` (new): the generic start's and resume's spec-keyed steps — slots, ssh slot files on the target, conversation (`LaunchConversation`, a mint past its deadline `DEADLINE_EXCEEDED`), role and jail refusals, and the session wiring (hooks, managed workflow, operator notify, a main's tool restriction; `TDDY_*` pairs go to the tool server).
- `opencode_cli_spawn` and the generic jail take them; `refuse_an_unsupported_cli_session_start` gates a managed codebase on `managed_workflow`, `refuse_a_type_main_cannot_run` on `project_home_main`.
- `HarnessLaunch` binds the conversation id, permission mode, `dangerously_skip_permissions` and `node_launcher`; `claude_template_params` and `cursor_template_params` deleted.
- `spec_launch::{launch_host_pty_without_inherited_tddy, in_a_pane_without_inherited_tddy}`: the generic host PTY and tmux pane run the agent without inherited `TDDY_*` variables.

See [module-layout.md](../module-layout.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-cli-spec-features.md).

# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

- `HostAgent.env_base`; the PTY launch runs the agent on the computed env (`PtyEnvBase::Cleared`, plus the PTY's own `TERM` / `COLORTERM`).
- `in_a_pane_with_only_the_agent_env` replaces `in_a_pane_without_inherited_tddy`; a pane keeps its own `TERM`, `COLORTERM`, `TMUX`, `TMUX_PANE` (`PANE_TERMINAL_VARIABLES`). `launch_host_pty_without_inherited_tddy` and `without_inherited_tddy_variables` are deleted.
- `ConversationStart.env_base`: the conversation-id mint starts from the agent env, host and sandboxed alike.

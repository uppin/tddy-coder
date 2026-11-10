# 2026-11-03 — The app jails `--agent <spec id>`

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- `--agent <id>` (`agent_spec::{choose_agent, agent_exec_plan, agent_exec_plan_with}`, `ChosenAgent`, `AgentJail`, `AgentSession`, `AgentChoiceError`, `TOOLS_SUBDIR`) replaces `--agent-kind` and `AgentKind`. The jail is lowered with `lower_tool_jail` and started through `--exec-plan`.
- Removed: `--claude-binary`, `--cursor-binary`, `--claude-home-dir`, `--cursor-home-dir`, the matching config keys, and the per-agent home seeding. The home is `<tddyhome>/homes/<key>`, and the default model is the spec's `models.default`.
- `host_agent::build_host_agent_argv` renders the spec's `host_agent` variant. On Linux an `--agent` other than `claude` is refused.

See [Sandboxed codebase mode](../../../../docs/ft/coder/sandboxed-codebase-mode.md).

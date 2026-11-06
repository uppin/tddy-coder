# 2026-11-02 — Headless Claude subagents render from the spec

**Type:** Feature

`#agent-tools` 6/12 ([#795](https://github.com/uppin/tddy-coder/pull/795)). `DaemonCodingAgentLauncher::with_coding_agents`; `for_daemon` installs the host's coding agents and plans headless `claude` through `headless_plan_from_spec`. A coding-agents load failure refuses those launches. `tddy-coding-agent-specs` moved to `[dependencies]`. See the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-headless.md).

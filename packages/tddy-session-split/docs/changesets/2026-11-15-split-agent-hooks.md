# 2026-11-15 — A split agent loads its hooks file and carries its spec's launch variables

**Type:** Fix

`split_hooks_file(session_dir)` names the hooks file in the split context dir
(`<context>/.claude/settings.local.json`), and `split_claude_extra_args` appends the
`host_agent` variant's hooks args for it, so start and resume both load the file.
The split agent's env carries the spec's `run.env` on start (`split_agent_pty.rs`,
through `agent_env`) and on resume (`SplitAgentWiring.env` from
`prepare_split_agent_wiring`).

Cross-package record: [docs/dev/changesets/2026-11-15-split-agent-hooks.md](../../../../docs/dev/changesets/2026-11-15-split-agent-hooks.md).

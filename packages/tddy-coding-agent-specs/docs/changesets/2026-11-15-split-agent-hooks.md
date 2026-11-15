# 2026-11-15 — A variant declares the flag that loads its hooks file

**Type:** Fix

`AgentVariant` gains `hooks_flag: Option<String>` — the flag that loads the hooks file
the daemon writes for a launch, for a variant whose own `args` keep the agent from
finding that file by itself. `CodingAgentSpec::variant_hooks_args(name, hooks_file)`
renders `[flag, path]` when the variant declares one, nothing when it declares none,
and refuses an undeclared variant as `variant_args` does. Claude's `host_agent` variant
declares `hooks_flag: --settings` in `claude.yaml`.

Cross-package record: [docs/dev/changesets/2026-11-15-split-agent-hooks.md](../../../../docs/dev/changesets/2026-11-15-split-agent-hooks.md).

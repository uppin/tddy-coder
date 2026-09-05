# 2026-07-21 — Write-capable subagent tools (`WRITE`/`STR_REPLACE`/`DELETE`)

**Type:** Feature

`SubagentTool` gains the opt-in mutation variants for coder-role defs; `CodebaseAccess::{write,str_replace,delete}` dispatch the exec-catalog shapes over `Managed` access only (`Local` returns a typed error — no unconfined host writes from a YAML field); `mutation_tool_definitions()` is kept out of the unfiltered FastContext tool list so read-only loops never advertise them. Feature [no-bash-mode.md](../../../../docs/ft/coder/no-bash-mode.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#308](https://github.com/uppin/tddy-coder/pull/308). (tddy-discovery)

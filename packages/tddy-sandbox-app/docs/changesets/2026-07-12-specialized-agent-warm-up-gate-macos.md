# 2026-07-12 — Specialized-agent warm-up gate (macOS)

**Type:** Feature

`run_macos` now calls `tddy_discovery::warmup::warm_up_agents` after resolving `specialized_defs` and before `spawn_claude_sandbox`: it prints a visible "waking N specialized agent(s)…" line, races the warm-up against `ctrl_c` (→ `exit(130)`), and on failure prints the error and returns `Err` so the in-jail agent CLI is never spawned — no fallback. Feature [specialized-subagents.md § Start-time warm-up gate](../../../../docs/ft/coder/specialized-subagents.md#start-time-warm-up-gate). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#296](https://github.com/uppin/tddy-coder/pull/296). (tddy-sandbox-app, tddy-discovery, tddy-daemon)

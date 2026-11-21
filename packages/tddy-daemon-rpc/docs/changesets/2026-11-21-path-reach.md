# 2026-11-21 — Orchestrator reads answered from a per-call reach

New `exec_tool/orchestrator_reach.rs`: an orchestrator-role session's `Read` / `Grep` / `Glob` are
answered from a `ReadReach` built per call (host project worktrees, agent `home_dirs`, session
grants). New dependency `tddy-actions`. Cross-package:
[docs/dev/changesets/2026-11-21-path-reach.md](../../../../docs/dev/changesets/2026-11-21-path-reach.md).

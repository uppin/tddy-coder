# 2026-11-15 — A split start fails when its agent exits during startup

**Type:** Fix

The split spawn records its agent's `main` terminal and watches it through the daemon's
startup window (`svc_spawn_split_agent/startup.rs`). An agent that exits inside the window
fails the start with its exit outcome and the tail of its output; the codebase half is torn
down by the existing agent-step error path.

Cross-package record: [docs/dev/changesets/2026-11-15-split-agent-liveness.md](../../../../docs/dev/changesets/2026-11-15-split-agent-liveness.md).

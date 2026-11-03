# 2026-11-03 — No agent argv sniffing

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- `build_allow_read_paths` grants the toolchain and the runner binary only, and the plan builders set `SandboxPlan.{context_dir, stdio}`. The agent-home helpers in `sandbox_session` are deleted.

See [task-and-action-services.md](../task-and-action-services.md).

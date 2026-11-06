# 2026-11-03 — The profile and spawn read the plan

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- The context-dir write deny reads `SandboxPlan.context_dir` (`profile::context_dir_of`). The stdio transport reads `SandboxPlan.stdio` (`spawn::stdio_mode_of`). Neither reads the runner's argv, and `context_dir_from_runner_argv` is deleted.

See [`tddy-sandbox` architecture](../../../tddy-sandbox/docs/architecture.md).

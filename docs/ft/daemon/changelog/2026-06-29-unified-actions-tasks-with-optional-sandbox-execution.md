# 2026-06-29 — Unified actions → tasks with optional sandbox execution

- New `tddy-actions` crate unifies subprocess, PTY, and pipeline execution behind `ActionSpec`; all long-running daemon work registers in the shared `TaskRegistry` (`ProcessRuntime`, `PtyRuntime`, `PipelineRuntime`)
- `actions.ActionService` RPC (`ListActionKinds`, `StartAction`, `GetAction`) complements existing `tasks.TaskService`; PTY terminals and action tasks appear in `ListTasks`
- Optional `ActionSpec.sandbox` runs confined process or runner-PTY actions via `sandbox_plan_builder` + `tddy-sandbox-recipes`; `SandboxSpec.cwd` and `extra_read_paths` wire working directory and read-only mounts; unsupported hosts return `failed_precondition`
- Session-action async jobs, `tddy-build` executor, fast tools, and sandboxed `tddy-coder` all share the same task model (`job_id == task_id`)
- Feature: [background-tasks.md](../background-tasks.md), [terminal-sessions.md](../terminal-sessions.md), [claude-cli-session.md](../claude-cli-session.md). PR [#244](https://github.com/uppin/tddy-coder/pull/244)

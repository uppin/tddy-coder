# 2026-11-03 — The runner runs plans, never agents

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- The agent modes and every agent flag are deleted (`spawn_claude_pty`, `spawn_cursor_pty`, `harness_pty.rs`). The runner refuses a start with no `--exec-plan`, `--pty-command` or `--workspace-tools`. `tests/runner_cli_surface.rs` pins it.
- `notify_when_the_process_exits` waits on a detached thread, so a runner whose server returned first is no longer held alive by its running process.

See [`tddy-sandbox` architecture](../../../tddy-sandbox/docs/architecture.md).

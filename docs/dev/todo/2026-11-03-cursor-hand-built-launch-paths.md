# 2026-11-03 — Cursor's hand-built launch paths outlive the spec path

**Category:** Deferred from `#agent-tools` 7/12 (#796) — deletions awaiting developer approval (CLAUDE.md § ASK)
**Source:** `/green` and `/validate-changes` on `feature/agent-tools/cursor-cli`; `TODO(cursor-cli)` at
`packages/tddy-agent-launch/src/cursor_cli_spawn.rs` (`prepare_cursor_cli_launch`, `spawn_cursor_cli_process`)

## What is there

Every Cursor launch now renders from `cursor.yaml` through the shared launchers: the host start and
resume through `launch_host_pty`, the jailed start through `launch_jailed` (`launch_jailed_cursor`).
A host without coding agents refuses with `FailedPrecondition`. The hand-built code below is no
longer reached by any production path, but is still compiled:

| Code | State |
|---|---|
| `packages/tddy-agent-launch/src/svc_start_sandboxed_cursor_cli_session.rs` | The file the changeset planned to delete. It still holds `start_sandboxed_cursor_cli_session` **and** the new `launch_jailed_cursor` / `write_jail_hooks`, which are live. **Recommendation: move `launch_jailed_cursor` and `write_jail_hooks` beside `claude_jail.rs` (e.g. a `cursor_jail.rs`) rather than deleting the file**; only the leftovers of the old per-agent body would then go |
| the per-agent bodies of `spawn_cursor_cli_session_inner` / `spawn_cursor_cli_session_reporting` and `resume_cursor_cli_session` | the `coding_agents: None` case now refuses at the top; what remains of the old hand-built shape is the worktree/metadata scaffolding shared with the spec path |
| `prepare_cursor_cli_launch`, `spawn_cursor_cli_process` (`cursor_cli_spawn.rs`) | dead; kept under `#[allow(dead_code)]` + `TODO(cursor-cli)` until the deletion is approved |
| `install_cursor_hooks_in_worktree` (`chat.rs`, `pub`) | no caller left in the workspace |
| `CliSessionManager::start_cursor` / `resume_cursor` (`packages/tddy-cli-sessions/src/cli_session_manager/launch.rs`) | no caller left on the daemon path |

## Why it was deferred

Deleting files and public functions needs developer approval, and the pipeline that greened #796
ran unattended. Nothing reaches these paths, so they cannot drift a running session; they only cost
reading time and keep the two code-issue records on `cursor_cli_spawn.rs` and
`svc_start_sandboxed_cursor_cli_session.rs` open.

## What would close it

With approval: delete the dead functions and the `#[allow(dead_code)]` markers, move the live jailed
launch out of `svc_start_sandboxed_cursor_cli_session.rs`, and re-measure
`packages/tddy-agent-launch/docs/code-issues/{oversized-file-cursor-cli-spawn,oversized-file-svc-start-sandboxed-cursor-cli-session,complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner,crap-svc-start-sandboxed-cursor-cli-session}.md`.
Natural owner: the `#agent-tools` sweep node (12/12), alongside
`2026-11-02-claude-hand-built-headless-paths.md`.

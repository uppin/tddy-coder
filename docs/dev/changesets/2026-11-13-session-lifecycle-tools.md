# 2026-11-13 — Orchestrators mark, pause, delete sessions and clear, delete, restore worktrees

**Type:** Feature

`#orchestrators 5/11`, PR [#882](https://github.com/uppin/tddy-coder/pull/882), stack
[#889](https://github.com/uppin/tddy-coder/stack/889). Feature:
[project-concept.md § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools).

## What changed

- **`tddy-changeset`** — `OrchestrationMark { mark, note, set_by_session_id, at_unix_ms }` and
  `OrchestrationMarkKind { Paused, Blocked, Done }` in `.session.yaml`
  (`SessionMetadata.orchestration_mark`); `clear_a_paused_mark` for the resume path.
- **`tddy-service`** — `SessionEntry.orchestration_mark = 37`; TypeScript regenerated.
- **`tddy-tool-engine`** — `project_home_lifecycle_tools.rs`: `SetSessionMark`, `DeleteSession`,
  `ClearWorktree`, `DeleteWorktree`, `RestoreWorktree`, `ProjectHomeLifecyclePort` with
  default-refusing methods; registered as project-home and target tools. `paused` records the
  mark then terminates an active target; resume clears a paused mark; `DeleteSession` refuses a
  running session; Clear/Delete resolve `path` against `ListWorktrees` and refuse an active
  owner; `DeleteWorktree` also refuses the project's home worktree whoever calls;
  `RestoreWorktree` is addressed by `session_id`.
- **`tddy-daemon-rpc`** — `exec_tool/project_home/lifecycle.rs`, the port's daemon impl over the
  Worktrees screen's functions; forwarding for all five.
- **`tddy-agent-launch`** — resume clears a paused mark (`svc_resume_session.rs`).
- **`tddy-session-activity`** — listing carries the mark.
- **`tddy-session-web`** — `SessionDrawerMarkBadge` (`sessions-drawer-item-mark-<id>`, note on
  hover) on the drawer row.
- **`tddy-tools`** — the closed-world admission list names the five tools.

## Resolved backlog

- Project-home worktree tools stop at reading: no Clear, Delete or Restore
  (`2026-10-25-project-home-worktree-clear-delete-restore`) — closed by `ClearWorktree`,
  `DeleteWorktree` and `RestoreWorktree` above, covered by
  `packages/tddy-daemon-rpc/tests/worktree_tools_acceptance.rs`. Its `docs/dev/todo/` entry is
  deleted by this wrap.

## Code issues

- `tddy-tool-engine/src/lib.rs` `oversized-file`: 890 → 891 production lines (one `pub mod`).
  Not split: stack-shared with #883 and later `#orchestrators` nodes (see LEFT_FOR_HUMAN in the
  PR wrap report).
- `packages/tddy-rust-typescript-tests/gen/session_pb.ts`: 2007 → 2058 lines — generated
  (`buf generate`) output, excluded from the 500-line budget.
- `packages/tddy-session-lifecycle/src/connection_service/agent_activity_unit_tests.rs`:
  1302 → 1303 production lines (one `orchestration_mark: None` literal). Not split: test-support
  file also touched by successor stack PRs.

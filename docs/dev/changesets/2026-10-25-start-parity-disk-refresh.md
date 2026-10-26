# 2026-10-25 — RefreshWorktreeDiskUsage recalculates sizes with progress

**Type:** Feature

`#start-parity` 8/9 ([#757](https://github.com/uppin/tddy-coder/pull/757)). A project-home session
can refresh worktree sizes with `RefreshWorktreeDiskUsage {path?, daemon_instance_id?}`, as the
Worktrees screen's Refresh does: it queues the size walk through `WorktreeSizeCalculator::enqueue`
for one worktree (membership-checked on git's own worktree list) or every worktree of the project,
and returns `{queued}` at once. Progress is read through `ListWorktrees` (`sizing {done, total}`) or
`GetWorktree` (`size_status`). Decision: **no ledger** — the calculator's persisted per-path state is
the record; the calculator and its concurrency are unchanged. Across-host calls ride the whole-call
forward unchanged. Across:

- `tddy-tool-engine` — `project_home_disk_refresh.rs`: definition, `RefreshQueued`, handler; `ProjectHomePort::refresh_worktree_disk_usage`, catalogue entry and dispatch in `project_home_tools`.
- `tddy-daemon-rpc` — `DaemonProjectHome::refresh_worktree_disk_usage`.
- `tddy-tools` — the project-home allowlist test names the tool.

Code issues (split deferred, stack-shared): `tddy-daemon-rpc/src/exec_tool/project_home.rs`
746 → 788, `tddy-tool-engine/src/project_home_tools.rs` 643 → 660, `tddy-tool-engine/src/lib.rs`
883 → 884. No TODO added.

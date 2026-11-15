# 2026-11-15 — The project-home tools can inspect and drive a split session

**Type:** Fix

A split session's agent half — a session with no worktree — is a target for the
project-home tools that act on the agent (terminal, activity, transcript, messages,
answers, terminate, delete, marks): `resolve_session_target` no longer refuses a
worktree-less record, and `SessionTarget.worktree` is an `Option` the worktree tools
ask for through `SessionTarget::worktree()`. Records carry their codebase half
(`TargetRecord.codebase`, from `SessionEntry`'s `codebase_*`), and the daemon's
project-home forward redirects a worktree tool (read, diff, restore, the git tools)
named with a split agent half's id and no host to its codebase half on the codebase
host, re-using the existing target-host forward (`is_worktree_target_tool` /
`retarget_args` in `tddy-tool-engine`). A session with neither a worktree nor a
codebase half is refused by the worktree tools as before. Stack `#split-alive` 3/4
([#907](https://github.com/uppin/tddy-coder/pull/907)); the product statement is in
[project-concept.md § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools).

This closes fix 5 of the backlog entry `2026-11-15-split-session-agent-not-running`
(deleted at the #905 wrap). What remains of that entry is recorded in
`docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md`, updated at this wrap to
mark fix 5 done in #907: why the agent exits (fix 4) is still unknown, and fixes 6–7
land in #908.

Validation at this wrap: `cargo fmt --check` and scoped `cargo clippy -p
tddy-worktree-service -p tddy-tool-engine -p tddy-daemon-rpc --all-targets -- -D
warnings` clean; scoped `./test -p tddy-worktree-service -p tddy-tool-engine
-p tddy-daemon-rpc` green except four pre-existing local failures on this Mac
(`remote_git_livekit_acceptance`, which needs `tddy-remote-git-repo` built, and the
three sandboxed-codebase suites in `tddy-daemon-rpc`, which fail on macOS
`/bin/cat: illegal option`); CI on Linux is the gate for those. File-length gate over
the PR range: `project_home_tools.rs` crossed the budget in this PR (493 → 501
production lines) and was decomposed at the wrap — the worktree read/diff tools moved
to `project_home_worktree_read.rs` by `tddy-tools restructure` (`move_item`, verified
with `restructure verify --against HEAD`: every statement accounted for), leaving
`project_home_tools.rs` at 372 and the new module at 144 production lines. No other
touched non-test source file is at or over 500 production lines. Code issues
re-measured: `complexity-exec-tool-ports-execute-tool` (`execute_tool`, 69 lines at
detection) is untouched by this PR — the redirect lives in `project_home_remote.rs`
beside `served_on_target_host`, not in `execute_tool` — and stays open as written; the
oversized-file records in `tddy-worktree-service` (`service.rs`, `worktrees.rs`,
`remote_git_service.rs`) name files this PR did not touch and stay open as written.

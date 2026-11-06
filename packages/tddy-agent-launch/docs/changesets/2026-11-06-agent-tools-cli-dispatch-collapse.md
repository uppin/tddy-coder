# 2026-11-06 — One launch path for every CLI agent; the claude and cursor dispatch is deleted

**Type:** Refactor

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)).

- Claude and Cursor start, resume, re-attach and run in a jail through the generic CLI spec launch (`opencode_cli_spawn`, `cli_harness_tmux`, `cli_harness_jail`, `svc_start_sandboxed_cli_harness_session`). `harness_from_spec::cli_launcher` returns `Option<CliSessionType>`; `recorded_cli_session_type` (re-attach, from `HostCodingAgents`, no `claude-cli` default); `child_session_type` (a child runs its parent's agent); `cli_branch_intent` (branch intent from `BranchNameRequired`). No coding agents → `FAILED_PRECONDITION`.
- `cli_session_pointer`: the `.tddy/session.json` pointer (managed socket and operator-notify session) written into the worktree at start and resume, owner-only and git-excluded in a checkout; removed on the agent's exit (PTY, or tmux with an exit hook) and on `DeleteSession`; written on an ssh target through the slot write; none in a jail. The agent process gets no `TDDY_*` variable.
- A split session resumes through the generic resume: `resume_cli_harness` re-derives its wiring first (`SplitSessions::resume_split_wiring`), resumes it in its context directory with no commit identity of its own, and applies the split flags and transport.
- `start_request_checks::runs_in_tmux` (the `-cli` suffix) replaces `TMUX_RUN_SESSION_TYPES`.
- Deleted: the `CliLauncher` dispatch, the agent-id constants, every bespoke claude/cursor launch, and the files `claude_jail.rs`, `resume.rs`, `svc_resume_sandboxed_claude_cli_session.rs` and `svc_start_sandboxed_cursor_cli_session.rs`. The full list, the unplanned deletions and the earlier nodes' code changed are in the cross-package entry.
- Code issues: 13 records closed, 4 narrowed, 9 regressed (kept), 4 added; restructuring is the sweep's (n20).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-cli-dispatch-collapse.md).

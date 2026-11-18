# 2026-11-18 — A failing session start on a target cannot be explained from what the daemon records

**Category:** Future enhancement
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

Both root causes of the smoke test's failures were found only by sampling `ps` on the Mac and on the
VM during a start. What the daemon records left them invisible:

- **No step log.** The daemon log has no line for the remote commands of a start: no argv, working
  directory, exit code or stderr, and nothing tied to the start id. `GetSessionStart` has a step and a
  status only.
- **Errors blame the wrong thing.** `no answer within 60s` does not say which command hung. A
  `chdir` failure is reported as the binary not running.
- **Static progress.** During the worktree step, `updated_at_unix_ms` stayed at the start time for
  ~80 s with `clone_progress: null`, so a slow fetch looked the same as a hang.
- **Failed starts record nothing they left behind.** Every failed start had `branch: null` and
  `worktree: null`, while each had cut a worktree and a branch on the workspace (five were left
  in `~/.tddy/repos/qape-hq/.worktrees/` on `remote-dev`).
- **`usable: true` means only reachable.** `DescribeSessionStartOptions` listed `remote-dev` as
  usable while neither harness could start there.

**Why deferred:** each is observability work, not a defect blocking a start once the blockers are
fixed.

**What would close it:** a per-start step log (host, argv, cwd, duration, exit or timeout, stderr
tail) readable through `GetSessionStart`. Record the branch and worktree as soon as they exist.
Name the failing step in every error. Document or widen `usable`.

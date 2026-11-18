# PRD: A failing start on a target says which command failed, and what it left behind

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 8/10 — branch `feature/coder-start/start-visibility`, base `feature/coder-start/target-agent-start`

## Problem

Both root causes of the 2026-11-18 smoke test's failures were found only by sampling `ps` on the
daemon host and the workspace during a start. What the daemon records left them invisible:

- **No step log.** Nothing records a start's remote commands: no command line, no duration, no exit
  code and no stderr. `GetSessionStart` shows a step and a status, nothing more.
- **Errors blame the wrong thing.** `no answer within 60s` does not say which command hung.
- **Progress looks static.** A slow fetch and a hang look the same: `updated_at_unix_ms` stood still
  for about 80 s.
- **Failed starts lose what they made.** Every failed start recorded `branch: null` and
  `worktree: null`, yet each had cut both on the workspace.
- **`usable` is misread.** `usable: true` means only *listed as startable*. `remote-dev` showed as
  usable while neither agent could start there.

## Requirements

1. **Per-start remote-call log.** `GetSessionStart` returns a bounded log of the start's remote calls,
   each with the host, the command (truncated, with secrets never included), the duration, and the
   outcome: exit code, timed out, or unreachable. A failed call also carries its stderr tail. Each
   call also advances `updated_at_unix_ms`.
2. **Timeouts name the command.** A one-shot timeout error names the command that timed out.
3. **Branch and worktree are kept.** They are recorded on the start as soon as the worktree is cut,
   and stay on the record when the start fails or is cancelled.
4. **`usable` is documented.** `DescribeSessionStartOptions` states what `usable` means: listed and
   startable (running, with the CLI installed). It is not a probe. This is a description change only.

## Acceptance criteria

- [ ] A start on a stand-in target lists its remote calls, in order, through `GetSessionStart`.
- [ ] A start whose worktree fetch fails reports that call's exit code and stderr tail, and the
      start's `error` names the step.
- [ ] A one-shot past its deadline fails with an error naming its command.
- [ ] A start that fails after the worktree cut still reports its `branch` and `worktree`.
- [ ] No session token or credential appears in the log.
- [ ] Scoped gates: `./test -p tddy-remote-shell -p tddy-tool-engine -p tddy-agent-launch -p tddy-daemon-rpc`
      and clippy with `-D warnings`.

## Out of scope

- A probe-based `usable`.
- Cleaning up the worktrees and branches a failed start left behind. Recording them is the first step.

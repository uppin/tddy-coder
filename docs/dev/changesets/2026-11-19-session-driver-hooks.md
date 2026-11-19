# 2026-11-19 — A driven session's hooks are followed as a daemon target start's are, and a follower leaves nothing running on its target

**Type:** Feature · **Packages:** `tddy-session-driver`, `tddy-remote-shell`, `tddy-agent-launch` ·
Backlog: `2026-11-16-coder-hosts-never-run-against-a-real-deployment` (narrowed, not closed)

## Problem

`tddy-session-driver` launched every agent with no rendered slots, so a driven session had no
hooks: nothing showed whether a target session's lifecycle hooks reach the daemon, which only a
daemon-level start could check. A live run on `mantasi/remote-dev` with the hooks wired then found
that the daemon's own hook-log follower leaves its `tail -f` running on the target after the
session ends: ending the follower's client does not end a remote command (ssh without a terminal
sends no hang-up), and `tail -f` on a log nobody writes to never notices its reader is gone.

## Backlog

- ℹ **Narrowed:** [`2026-11-16-coder-hosts-never-run-against-a-real-deployment`](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)
  — its last open item, "the hook follower on a Coder workspace", ran live here: the daemon's own
  follower and mapping, through the real `coder` CLI, delivered Claude's and Cursor's hooks with the
  daemon's reports. What remains is the daemon's delivery of those reports into its activity
  service on a live workspace, which the driver replaces.

## Change

- **`tddy-session-driver` follows a session's hooks.** For an agent whose spec declares the `hooks`
  capability, a start wires them exactly as a daemon start on a target does, on every target — a
  local one included: a fresh hook token; the hook slots rendered with
  `HookCommandParams::remote_log = REMOTE_HOOK_LOG`, so each hook is the shell one-liner that
  appends to `.tddy/hooks.log` in the worktree; the log created with `create_remote_hook_log`; a
  `RemoteHookFollower` started through the session's shell once the agent is up, registered in a
  `RemoteHookFollowers`; and each line mapped by `ActivityHookSink`. The session's exit stops the
  follower and removes the log; a stop waits for both. A start that fails removes the log it
  created. Only the delivery differs: `hooks.rs`'s `HookReportDelivery` records each
  `ReportSessionStatus` / `ReportAgentActivity` request beside the hook that raised it.
- **Two MCP tools.** `read_hooks {since?}` → `{followed, hooks, next}`, each hook
  `{index, event, payload, reports}` with reports as `{kind: session_status | agent_activity, …}`
  (no hook token). `wait_for_hook {event, since?, timeout_secs?}` → `{matched, hook}`.
- **`tddy_remote_shell::LocalShell`.** This machine as a transport (`/bin/sh -c`, tag `local`),
  held to the crate's contract suite; it is what a local driven session's log is followed through.
  `SshShell` and `LocalShell` share one module for running a child to its end under a deadline and
  for streaming one (`child.rs`).
- **`remote_hook_follower::follow_command`** ends its `tail` once the log is removed:
  `tail -c +N -f '<log>' & t=$!; (while [ -e '<log>' ]; do sleep 1; done; kill "$t") >/dev/null 2>&1 & wait "$t"`.
  The watch is a background subshell, so it outlives the waiting shell being killed with its client.
  Every session end already removes the log, so no follow outlives its session.

## Acceptance tests

- `tddy-session-driver` `session_driver_hooks_acceptance`: a Coder session's hooks arrive with the
  daemon's reports (`SessionStart` → `Started`, `PostToolUse` → `ExecutingTool` + agent activity,
  `Stop` → `Done`), a forged line is dropped, the log is read through `coder ssh --stdio`, and after
  the stop the log is gone and nothing still follows it; the same for a local session; `since`;
  a wait that times out; an agent without hooks has none followed. The driver's fixtures moved to
  `tests/support/`; the stand-in `ssh` runs its command as a child, as a real ssh session does.
- `tddy-agent-launch` `remote_hook_log_tail_ends`: a follow's `tail` ends once the log is removed,
  though its client was killed — red before the fix.
- `tddy-remote-shell` `shell_contract`: `LocalShell` behaves like a remote shell, and a dropped local
  stream ends its command.

## Live check (remote-dev, Coder v2.33.5)

Through this branch's driver: Claude — `SessionStart` (Started), `UserPromptSubmit` (Running),
`PostToolUse` (ExecutingTool, activity `Bash`), `Stop` (Done); Cursor — `sessionStart`,
`beforeSubmitPrompt`, `preToolUse` (activity `Shell`), `postToolUse`, `stop`. Before the
`follow_command` fix each session left a `tail -c +1 -f …/hooks.log` on the workspace; after it,
none.

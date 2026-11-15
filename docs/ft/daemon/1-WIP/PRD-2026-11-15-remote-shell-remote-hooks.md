# A remote agent's hooks reach the daemon with nothing installed - PRD

**Date**: 2026-11-15
**PRD Type**: Enhancement
**Stack**: `#remote-shell 6/7` — branch `feature/remote-shell/remote-hooks`, base `feature/remote-shell/derived-hosts`

## Affected Features

- **Agent session status**: [agent-session-status.md](../agent-session-status.md) — status for a session whose agent is on a target
- **Claude CLI session**: [claude-cli-session.md](../claude-cli-session.md) — hooks on a target session
- **Cursor CLI session**: [cursor-cli-session.md](../cursor-cli-session.md) — hooks on a target session
- **Muse CLI session**: [muse-cli-session.md](../muse-cli-session.md) — hooks on a target session

## Summary

When a session's agent runs on a target, its lifecycle hooks still reach the daemon, with no tddy software on the target. Each hook is a plain shell command that appends one line to a file in the session's worktree; the daemon follows that file through the remote shell. It works for every agent whose spec supports hooks.

## Background

A hook today runs `tddy-tools session-hook`, which needs the `tddy-tools` binary and a route to the daemon. A target has neither, and installing anything there is ruled out.

Every hooks adapter (Claude Code's settings file, Cursor's hooks JSON, Muse's `-c hooks=…`) builds the same shell command string from one `HookCommandParams`. So the carrier is a property of the command, not of the agent: replace the command and every adapter, present and future, is covered.

The user chose notify-only on 2026-11-15: a hook reports, and nothing is sent back. Hooks that wait for an answer are therefore not installed on a target.

## Proposed Changes

### What's Changing

- On a target session the hook command is a shell one-liner using only `sh`, `printf`, `cat` and `tr`: it appends `<event> <token> <payload on one line>` to `.tddy/hooks.log` in the worktree.
- The daemon follows that file through the session's remote shell from the moment the agent starts, and feeds each line into the same handling a local hook reaches.
- It applies to every agent whose spec declares the hooks capability. File-delivered hooks are written on the target; argument-delivered hooks join the remote command line.
- Hooks that wait for an answer (permission requests, the question tool) are not installed on a target. The agent asks in its terminal.
- A line with the wrong token, an unknown event or an unparseable payload is dropped and counted, never acted on.
- If the follower's connection drops, it resumes from the byte it had reached, so no hook is lost or delivered twice.

### What's Staying the Same

- Local sessions keep `tddy-tools session-hook`, including the hooks that wait for an answer.
- The six agents with no hooks support launch on a target and report nothing, as they do locally.
- The hook events an adapter installs, and what each means, are unchanged.

## Impact Analysis

### Technical Impact

- `HookCommandParams` gains a carrier.
- A follower task per target session, on the port's streamed exec.
- The worktree gains `.tddy/hooks.log`, excluded from git like the pointer.

### User Impact

- A target session shows working, waiting and idle like a local one.
- Its permission prompts and questions appear in the terminal, not in the web's prompt UI.

## Implementation Plan

1. The remote command and the carrier, per adapter.
2. The follower and its ingestion.
3. Resume from offset, bad lines, cleanup.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] A hook fired by an agent on a target updates the session's activity on the daemon
- [ ] The hook command on a target names no tddy binary and uses only `sh`, `printf`, `cat` and `tr`
- [ ] Claude Code, Cursor and Muse sessions on a target each deliver hooks
- [ ] No hook that waits for an answer is installed on a target
- [ ] A line with a wrong token, an unknown event or a broken payload is dropped and does not change the session
- [ ] After the follower reconnects, every hook is delivered exactly once, in order
- [ ] A payload containing newlines and quotes arrives intact
- [ ] Stopping or deleting the session stops the follower and removes the log

## References

### Affected Features (Complete List)

- **Agent session status**: [agent-session-status.md](../agent-session-status.md) — status for a session whose agent is on a target
- **Claude CLI session**: [claude-cli-session.md](../claude-cli-session.md) — hooks on a target session
- **Cursor CLI session**: [cursor-cli-session.md](../cursor-cli-session.md) — hooks on a target session
- **Muse CLI session**: [muse-cli-session.md](../muse-cli-session.md) — hooks on a target session

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-remote-hooks.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`

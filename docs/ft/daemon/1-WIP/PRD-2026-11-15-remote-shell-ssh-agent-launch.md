# A session whose agent runs on an SSH host - PRD

**Date**: 2026-11-15
**PRD Type**: Enhancement
**Stack**: `#remote-shell 3/7` — branch `feature/remote-shell/ssh-agent-launch`, base `feature/remote-shell/coder-account`

## Affected Features

- **Claude CLI session**: [claude-cli-session.md](../claude-cli-session.md) — the ssh checkout section gains an agent that runs on the target
- **Terminal sessions**: [terminal-sessions.md](../terminal-sessions.md) — a session terminal whose process is on another machine
- **Remote managed worktree**: [remote-managed-worktree.md](../remote-managed-worktree.md) — the existing `ssh_config_host` mode is unchanged and is now one of two SSH modes

## Summary

A session can name a **target**: a machine the daemon reaches through a remote shell, with no tddy software on it. The daemon creates the worktree there, starts the coding agent there in an interactive shell, and serves its terminal as it serves a local one. SSH hosts are the first kind of target.

## Background

Remote sessions exist today only between daemons over LiveKit. The one SSH mode, `ssh_config_host`, runs tools on the target while the agent stays in a local PTY under managed-codebase rules; `launch_host_pty` has no SSH branch, and `record_remote_owner` waits for a launch that was never written.

The daemon as mediator removes the need for a daemon on the far side: the session is an ordinary local session whose PTY child is the transport's client. The serving daemon keeps the terminal, the capture, the control lease and the session record.

## Proposed Changes

### What's Changing

- `StartSessionRequest.target` names where the agent runs. Its first case is an SSH config alias of the serving daemon's user.
- The worktree is created on the target, as an SSH checkout's is today.
- The agent is started on the target inside an interactive remote shell, in the worktree, with the environment its spec declares.
- The session's terminal, history, resize, input and stop work as for a local session. Stopping the session ends the remote agent.
- The session record and `.session.yaml` carry the target. The session pointer on the target names the remote agent as its owner.
- The agent uses its own file and shell tools on the target. Managed codebase, a sandbox, a split placement and the project-home main are each refused together with a target, with the reason.
- A resume starts the agent on the target again with its spec's resume arguments.

### What's Staying the Same

- `ssh_config_host` keeps its meaning: agent local, tools remote, managed-codebase semantics.
- LiveKit peer starts (`daemon_instance_id`) are untouched.
- No web change in this PR; a target is sent by RPC clients and by later PRs' web work.

## Impact Analysis

### Technical Impact

- `session.proto`: `SessionTarget`, `StartSessionRequest.target`, `SessionEntry.target`.
- `tddy-agent-launch`: a remote branch in the CLI harness launch.
- `tddy-changeset`: `.session.yaml` gains `target`.

### User Impact

- Through the API, a session can run entirely on an SSH host that has only the agent CLI and `git`.
- Permission prompts and questions are answered in the terminal for these sessions.

## Implementation Plan

1. Target model: proto, metadata, validation and refusals.
2. Remote launch: command composition, local PTY, startup watch, pointer owner.
3. Stop, exit, delete and resume.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] A start with an SSH target runs the agent on that host in the session's worktree, and its output reaches the session terminal
- [ ] The agent's environment on the target is its spec's allow-list and `run.env`, nothing inherited from the daemon
- [ ] Stopping the session ends the remote agent; the agent exiting ends the session's terminal with its exit status
- [ ] The session record carries the target, and the pointer on the target names the remote agent's pid and host
- [ ] A target combined with managed codebase, a sandbox, a split placement or the project-home main is refused with the reason
- [ ] An agent that exits within the startup window fails the start with its last output
- [ ] A resume starts the agent on the target again
- [ ] An unreachable target fails the start with the shell's reason and leaves no session

## References

### Affected Features (Complete List)

- **Claude CLI session**: [claude-cli-session.md](../claude-cli-session.md) — the ssh checkout section gains an agent that runs on the target
- **Terminal sessions**: [terminal-sessions.md](../terminal-sessions.md) — a session terminal whose process is on another machine
- **Remote managed worktree**: [remote-managed-worktree.md](../remote-managed-worktree.md) — the existing `ssh_config_host` mode is unchanged and is now one of two SSH modes

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-ssh-agent-launch.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`

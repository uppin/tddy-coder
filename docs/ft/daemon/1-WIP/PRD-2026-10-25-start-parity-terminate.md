# PRD: TerminateSession stops a session as the UI's Terminate does

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 7/9 — branch `feature/start-parity/terminate`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

An orchestrator cannot stop a session it started. This node adds **`TerminateSession`**, which
does exactly what the UI's Terminate does.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

`TerminateSession {session_id, daemon_instance_id?}` calls `SignalSession` with SIGTERM on the
target's host. The description says the process gets SIGTERM, the session's worktree, branch and
record are kept, a stopped session can be resumed, and terminating an already-stopped session is an
error. It is limited to the caller's project.

### What stays the same

Does not expose `DeleteSession` (which removes the worktree), SIGINT or SIGKILL. Recorded as a TODO if asked for.

## Impact

- **Technical:** tddy-tool-engine, tddy-daemon-rpc.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] An orchestrator can terminate a session in its project on any host, with the same effect as the UI's Terminate.
- [ ] The tool says the worktree and the branch are kept.

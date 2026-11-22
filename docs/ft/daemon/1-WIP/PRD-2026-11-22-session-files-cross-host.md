# An orchestrator's session files reach the sessions it starts on other hosts - PRD

**Date**: 2026-11-22
**PRD Type**: Enhancement
**Stack**: `#orchestra` 10/11 — branch `feature/orchestra/session-files-cross-host`, base `feature/orchestra/orchestrator-prompt`

## Affected Features

- **Primary Feature**: [Session attachments](../../coder/session-attachments.md) — session files an orchestrator writes
- **Related Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

A session file an orchestrator writes (`WriteSessionFile`, `#orchestra` 3/9) can be attached to a session it starts on **any** eligible host, not only its own: the attachment it is answered with names the orchestrator's daemon, so the child's host fetches the file from there. And a session-file call that reaches a daemon which does not hold the calling orchestrator is served by the daemon that does, as the other project-home tools are.

## Background

`#orchestra` 3/9 serves the two tools on the orchestrator's own host only. `attachment_for` answers `{basename, scope: SESSION_ARTIFACT, session_id, relative_path}` with no `daemon_instance_id`, so a `StartSession` on another host looks for the file in its own sessions base and does not find it. Recorded as `docs/dev/todo/2026-11-21-session-file-tools-are-not-forwarded-to-a-peer-host.md`; the developer chose to continue it in this stack at the TODO gate (2026-11-22).

## Proposed Changes

### What's Changing
- `WriteSessionFile`'s `attachment` names the daemon that holds the file (`daemon_instance_id`), so the attachment works from a `StartSession` targeting any host.
- A `WriteSessionFile` / `ListSessionFiles` call for an orchestrator held on another daemon is served there: the daemon that receives it confirms the caller at its owner (as the project-home tools already do) and forwards it to that owner; the file always lands in the caller's own session directory.

### What's Staying the Same
- The file layout (`artifacts/files/<name>`), name and size rules, and the worker-session refusal.
- How a started session materializes an attachment from another host (`ReadHostDocument` with a `daemon_instance_id`).

## Impact Analysis

### Technical Impact
tddy-tool-engine (`attachment_for` takes the holding daemon), tddy-daemon-rpc (the session-file tools join the caller-held-elsewhere route).

### User Impact
An orchestrator can hand a written brief to a child it starts on another machine.

## Acceptance Criteria

- [ ] The attachment `WriteSessionFile` answers names the orchestrator's daemon.
- [ ] A session started on another host with that attachment receives the file.
- [ ] A session-file call that lands on a daemon not holding the orchestrator writes into the orchestrator's own session directory on its owner.
- [ ] A worker session's call is still refused, from any host.

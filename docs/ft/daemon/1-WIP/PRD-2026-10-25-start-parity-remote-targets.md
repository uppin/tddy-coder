# PRD: list and reach sessions on every host

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 1/9 — branch `feature/start-parity/remote-targets`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

A project-home session can only see and act on sessions of its own daemon. Its project's
sessions on other registered hosts are invisible to `ListProjectSessions`, and every target tool
(`ReadSessionWorktree`, `DiffSessionWorktree`, the seven git tools) refuses them. This node makes
the orchestrator's reach span every eligible host: the list fans out across hosts, and every
project-home tool call that names a target on another host is forwarded to that host and run
there.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

- `ListProjectSessions` lists this host's sessions of the project, then asks every other eligible
  daemon (`ListEligibleDaemons`' roster) for its own, by forwarding the same tool call. It merges
  the answers into `ListProjectSessionsResult { sessions, unreachable_hosts }`. Each row carries
  its `daemon_instance_id`. A host that does not answer is named in `unreachable_hosts` with the
  reason, never silently dropped. A call that arrived from a peer lists only that peer's own
  sessions, so the fan-out never recurses.
- Every target tool (read, diff, the seven git tools) takes an optional `daemon_instance_id`
  argument (`TARGET_DAEMON_INSTANCE_ID`; empty = this host). On a call naming another host, the
  home session's daemon re-issues the same `ExecuteToolRequest` with
  `ExecuteToolRequest.daemon_instance_id` set, through the existing `rpc_served_by_peer` forward.
  The tool runs **on the target's host, unchanged**: that host's path containment, activity gate
  and push identity apply.
- **The caller is confirmed where it lives (no proto change).** A project-home tool call naming a
  `session_id` this daemon does not hold is served only after the daemon that **owns** that session
  confirms it:
  - ask each eligible daemon `session.SessionService/ListSessions` with the caller's token, over
    the common room;
  - accept only the owner's row, and only with `session_role == "project-home"`;
  - its `project_id` becomes the caller's project for target resolution.

  An agent cannot forge this: a request it addresses at a peer itself (`ExecuteTool` forwards
  before authenticating) is confirmed at the owner, which reports its real role.

### What stays the same

- Does not start sessions anywhere (that is `start-tool`) and adds no new tool.
- Does not forward the worktree, activity or terminate tools. Those nodes add their tools and each
  proves its own cross-host case through this node's forward.
- Does not change `ListSessions` (the RPC) or how the web fans out.

## Impact

- **Technical:** tddy-daemon-rpc, tddy-tool-engine, tddy-worktree-service.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] `ListProjectSessions` lists the project's sessions on every eligible host, each with its host.
- [ ] An unreachable host is reported, not silently omitted.
- [ ] Read, diff and every git tool work on a session on another host.
- [ ] A target outside the orchestrator's project is refused on whichever host holds it.
- [ ] A tool call from a session that is not a project-home session is refused on every host; the host that owns the calling session is the one that confirms its role.

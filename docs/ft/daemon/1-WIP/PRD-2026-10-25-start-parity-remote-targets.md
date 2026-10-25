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

- `ListProjectSessions` asks every eligible daemon (the roster `ListEligibleDaemons` serves)
  for the project's sessions and merges the answers. Each row carries `daemon_instance_id`, and a
  host that does not answer is reported in an `unreachable_hosts` list, never silently dropped.
- Every target tool's arguments gain an optional `daemon_instance_id` (empty = this host). A call
  naming another host is forwarded to it as an `ExecuteToolRequest`, carrying a **delegated home
  caller**: the home session id, its project and the home daemon's id.
- The peer serves a project-home tool for a delegated caller without reading the home session's
  record. It still checks that the target is in the asserted project and that the caller's token
  maps to the same OS user, and it applies its own path containment, activity gate and push identity.
- The delegated caller is accepted **only** on a request that arrived over the common room from the
  daemon it names. A delegated caller in a request arriving over `/rpc` or IPC is refused.

### What stays the same

- Does not start sessions anywhere (that is `start-tool`) and adds no new tool.
- Does not forward the worktree, activity or terminate tools. Those nodes add their tools and each
  proves its own cross-host case through this node's forward.
- Does not change `ListSessions` (the RPC) or how the web fans out.

## Impact

- **Technical:** tddy-service (exec_tools.proto), tddy-daemon-rpc, tddy-tool-engine, tddy-worktree-service.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] `ListProjectSessions` lists the project's sessions on every eligible host, each with its host.
- [ ] An unreachable host is reported, not silently omitted.
- [ ] Read, diff and every git tool work on a session on another host.
- [ ] A target outside the orchestrator's project is refused on whichever host holds it.
- [ ] A delegated caller is honoured only from a peer daemon over the common room.

# PRD: spawn_conversation starts any session through the universal start

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 9/9 — branch `feature/start-parity/spawn-conversation`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

`spawn_conversation` is a second, narrower start path: grill-me only, claude-cli only, local
only, with no progress and no cancel. This node puts it on the universal start. It takes
`StartSession`'s arguments, goes through `start_session_on_request`, and returns the async handle.
Sessions that can spawn conversations are also offered the start tools, limited to their own
project.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

- `spawn_conversation` (MCP and CLI) takes `StartSessionArgs` (from `start-tool`), with
  `stack_parent` defaulting to the caller.
- The handler builds a `StartSessionRequest` and runs it through the async start (`async-start`),
  so it can start any session type on any eligible host, with progress and cancel. It returns
  `{start_id, task_id, daemon_instance_id}`.
- New tddy-tools subcommands `spawn-conversation-status` and `spawn-conversation-cancel`, and
  their MCP counterparts.
- What is offered: a session with the conversation-spawn capability is offered `StartSession`,
  `GetSessionStart`, `CancelSessionStart` and `DescribeSessionStartOptions`, with `project_id` fixed
  to its own project. The grill-me binding is kept as the capability's *source*, not as a limit on
  what can be started.

### What stays the same

- Does not widen which recipes get the spawn capability.
- Does not change the start tools' schemas (those belong to `start-tool`, `async-start` and
  `start-options`).

## Impact

- **Technical:** tddy-agent-launch, tddy-tools, tddy-host-service, tddy-tool-engine, tddy-daemon-rpc.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] `spawn_conversation` can start any session type, on any eligible host, with every New-session option.
- [ ] It returns a handle, and the caller can poll and cancel the start.
- [ ] A session that can spawn conversations is offered the start tools, limited to its own project.

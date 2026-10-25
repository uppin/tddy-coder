# PRD: DescribeSessionStartOptions lists a host's agents, models and choices

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 5/9 — branch `feature/start-parity/start-options`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

Several of `StartSession`'s choices depend on the host and on each other: agents, models per
agent and provider account, provider routes, SSH hosts, subagents, branches, and stack-parent
candidates. The form fetches them live. A static schema cannot hold them without going stale, so
this node adds **`DescribeSessionStartOptions`**, which answers them from the same RPCs the form uses.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

`DescribeSessionStartOptions {daemon_instance_id?, session_type?, agent?, provider_account_id?}`
→ `{agents, models: {default, choices}, provider_routes, ssh_config_hosts, subagents, branches,
stack_parents, pr_stack_base_candidates}`.
- Each list comes from the RPC the form calls, run on the named host. A remote host is reached
  through `remote-targets`' forward.
- The stack filters (`stackParents.ts`) are mirrored in Rust and pinned by a fixture shared with a
  Cypress spec.

### What stays the same

- Does not change `StartSession`'s schema. The host enum stays the `start-tool` node's.
- Adds no RPC.

## Impact

- **Technical:** tddy-tool-engine, tddy-daemon-rpc.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] An orchestrator can find the agents, models, provider accounts, SSH hosts, subagents, branches and stack parents it may choose for a start on any host, the same ones the form offers.

# PRD: StartSession takes every New-session option, on any host

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 2/9 — branch `feature/start-parity/start-tool`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

`StartProjectSession` exposes five of the New-session form's ~30 options and can only start on
its own host. This node replaces it with **`StartSession`**. The tool's arguments are
`StartSessionRequest`'s own fields, its schema is generated, and two parity tests keep it in step
with the proto and with what the form can send. The orchestrator link moves into the request, so
the daemon that creates the session records it, on any host.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

- `tddy-tool-engine/src/start_session_args.rs`: `StartSessionArgs`, whose serde names are
  `StartSessionRequest`'s field names.
  - It has every field the form can send.
  - `attachments` covers host documents only.
  - `project_id` and `session_token` are filled in by the caller's context.
  - `on_branch_conflict` is fixed to `"reject"`.
  - `INTERNAL_START_FIELDS` names each proto field the tool withholds, with a reason.
- The schema is generated from the args model:
  - enums for session types, recipes (`supported_workflow_recipe_cli_names()`), permission modes
    and branch intents;
  - `if/then` blocks that state required-ness per session type and per intent;
  - a description on every property.
- The host enum is filled in when `ListExecTools` answers, from the eligible-daemon roster.
- New `StartSessionRequest` fields: `orchestrator_session_id = 44` and
  `orchestrator_daemon_instance_id = 45`. The start core records them in the new changeset on the
  daemon that creates the session, so a forwarded start is tagged on the peer. `tag_orchestrator`
  is removed.
- The tool is renamed `StartProjectSession` → `StartSession`, with no alias. It is still
  synchronous here; `async-start` makes it return a handle.

### What stays the same

- Does not make the start asynchronous, and adds no status or cancel tools (that is `async-start`).
- Does not add the per-host discovery lists (`start-options`).
- Does not touch `spawn_conversation` (`spawn-conversation`).
- Does not change the form's behaviour. The form's drift (8 recipes vs. the server's 10) is
  recorded as a TODO.

## Impact

- **Technical:** tddy-service (session.proto), tddy-tool-engine, tddy-daemon-rpc, tddy-agent-launch, tddy-web (parity spec only).
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] An orchestrator can set every option the New-session form offers.
- [ ] The schema lists closed choices as enums and says which fields each session type requires.
- [ ] A start naming another eligible host creates the session there, recorded as started by the orchestrator.
- [ ] A new request field, or a new form option, the tool lacks fails a test.

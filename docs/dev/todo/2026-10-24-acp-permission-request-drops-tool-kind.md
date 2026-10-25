# 2026-10-24 — The ACP driver's permission request drops the tool kind

**Category:** Deferred from `coagents` (#coagents 3/8, `permission-yield`, #722)

**Source:** [`2026-10-24-coagents-permission-yield.md`](../changesets/2026-10-24-coagents-permission-yield.md)
— the `PermissionToolCall.kind` of a coding subagent's yielded request.

## What is wrong

`tddy_agent_backend::PermissionRequest` (`packages/tddy-agent-backend/src/backend/acp_driver.rs`)
carries `session_id`, `tool_call_id`, `title`, `raw_input` and `options`, but no tool kind.
`acp_driver/mapping.rs::permission_request` builds it from the ACP `session/request_permission`
request and drops `request.tool_call.fields.kind`, which the agent sends (`edit`, `execute`, `read`, …).

So the coding subagent's yielded request reports `kind: "other"` for every tool call
(`packages/tddy-coding-subagent/src/parked_permissions.rs`, `view_of`, FIXME).

## Why it is deferred

Closing it changes `PermissionRequest`, the driver's own surface (#720, `acp-driver`). The
permission-yield node is not allowed to change the driver API, so the gap is recorded here instead of
patched in the consumer. The answer itself is unaffected: a caller answers by `option_id`, and the
turn works without the kind. Only the caller's view of what the agent asked is less informative.

## What closes it

An additive field on `PermissionRequest` (an `Option` of the ACP kind, or its name), filled in
`mapping::permission_request` from `tool_call.fields.kind`. Then `view_of` passes it through as
`PermissionToolCall.kind`, and the FIXME in `parked_permissions.rs` is removed. Construction sites to
update with the field: `mapping.rs` and the test literal at `acp_driver.rs` (`fn asking`).

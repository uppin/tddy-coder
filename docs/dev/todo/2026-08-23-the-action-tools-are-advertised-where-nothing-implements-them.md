# 2026-08-23 — The action tools are advertised where nothing implements them

**Category:** Future enhancement
**Status:** Resolved
**Source:** seeded-agents-on-any-placement changeset, 2026-08-23

`tddy-tools` merges `action_tool_router()` — `request_action`, `list_actions`, `invoke_action` — into
its router whenever a session-tool transport is reachable (`packages/tddy-tools/src/server.rs:313`),
which for a daemon-hosted `claude-cli` session it always is. That session's dispatch lands in
`tddy_tool_engine::execute_tool_with_env` (`packages/tddy-tool-engine/src/lib.rs:217-234`), whose
`match` has no arm for any of the three, so every call comes back
`{"error":"unknown tool: ListActions","is_error":true}`. Only `tddy-sandbox-app`'s bridge and the
`tddy-coder`-hosted `toolcall::listener` implement them.

Not merely dead surface: it misleads. In session `01a03066`, asked to invoke a specialized agent, the
main agent reached for `invoke_action`, got `unknown tool: InvokeAction`, and reported that the agent
was not registered — a wrong conclusion drawn from a tool that should not have been on offer. Either
implement the three on the daemon path (they are host round-trips: `EstablishAction`, `ListActions`,
`InvokeAction` against the session directory) or gate the merge on a transport that serves them.

## Resolved — withdrawn, 2026-09-10

`unbundle-tools-thinning` (#unbundle node 5, [PR #474](https://github.com/uppin/tddy-coder/pull/474))
took the second option, at M5. Implementing the three on the daemon path would have been three new
daemon round-trips — a feature, in a node whose whole claim is that no behaviour changed.

The gate as this entry phrased it turned out not to be expressible, and that is the substantive
finding: **"a transport that serves them" is not a property of the transport.**
`SessionToolTransport::SandboxIpc` is the transport for `tddy-sandbox-app`'s `AppToolHandler`,
which implements all three, *and* for `tddy-daemon`'s `DaemonToolHandler`, which implements none —
and the in-jail server cannot tell the two apart from the socket. So the **host** says so, with
`TDDY_SESSION_ACTION_TOOLS` (`tddy_core::session_actions::SESSION_ACTION_TOOLS_ENV`), the same
shape as the `TDDY_LSP_TOOLS` gate one line below it in the same router.
`tddy-sandbox-app`'s spawn path sets it; the daemon path does not.

Verified over the real `--mcp` stdio wire and pinned as a regression test:
`packages/tddy-tools/tests/mcp_tool_advertisement_audit.rs` asserts the whole advertised set by
name on both paths — **43 tools where the host claims the surface, 40 where it does not**,
differing in exactly `request_action`, `list_actions` and `invoke_action`. The three that go are
the three that answered `{"error":"unknown tool: ListActions","is_error":true}` to every call.

# complexity: request_approval

**Location:** `packages/tddy-session-tool-client/src/approval.rs` — `request_approval`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` step 4 (clean-code scan, fn line to closing brace) on PR #726 (`#coagents` 7/8)
**Metrics:** **104 lines** · nesting 3 · 3 parameters · budget 60 / 4
**Thresholds breached:** length 104 > 60
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-25 | 104 | 3 | first detection — new in PR #726: one match arm per `SessionToolTransport`, each building a `SessionToolEnvelope` and connecting a client. Deferred to the `#coagents` sweep node by the developer (2026-10-25) |

## What would close it

The arms repeat the transport → (client, envelope) resolution that `dispatch_session_tool`
(`src/lib.rs`, 70 lines) also performs. One shared `connect_session_tool_client(transport) ->
Result<(Arc<dyn RpcClientTransport> | DaemonHttp, SessionToolEnvelope), String>` used by both
removes the duplication and leaves `request_approval` a dispatch of a few lines.

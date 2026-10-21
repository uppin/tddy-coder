# panicking-api: opencode_config_for

**Location:** `packages/tddy-agent-backend/src/provider_route.rs` — `opencode_config_for`
**Category:** panicking-api
**Detected:** 2026-10-22 by `structural audit` (#provider-route 3/9, `[G]` green of M3)
**Metrics:** 1 reachable `panic!` in a `pub` function · 3 pinned call sites (`tests/opencode_route_acceptance.rs`) · 0 production callers outside `resolve_route`
**Restructure:** not required — ordinary work, changes a pub signature
**Status:** Open — **unclaimed**

## Measurement history

| Run | Panics | Production callers | Note |
|---|---|---|---|
| 2026-10-22 | 1 | 0 outside `resolve_route` | first detection; the `resolve_route` path was made panic-free by a typed wire selection in the same node |

## What the tool found

`opencode_config_for(&RouteTarget) -> RouteFile` panics when the target's account serves neither
OpenAI Responses nor OpenAI Chat Completions. The `resolve_route` arm no longer reaches it: it
selects the wire through a private typed value and refuses such an account with
`RouteRefusal::ProtocolNotServed`. The pub function is kept as a thin wrapper over the same private
builder because the contract tests pin its signature (`RouteTarget` in, `RouteFile` out) and node
7 (`opencode-acp`) is drafted against it.

## Why it matters here

Node 7 reuses `opencode_config_for` from a headless tool-session path. A caller that does not go
through `resolve_route` first can hand it an Anthropic-only account and take the daemon's task down
with a panic instead of a refusal.

## What would close it

Make the bad input unrepresentable at the pub boundary: take the validated wire type (or return
`Result<RouteFile, RouteRefusal>`), and update the three pinned call sites and node 7's draft
contract together. Cheapest to do in node 7, before it adds a caller.

## Verified by hand

2026-10-22: read `opencode_route` and `opencode_config_for`; the only production caller is
`resolve_route`'s OpenCode arm, which is guarded by `route_supported` and now by the typed wire.

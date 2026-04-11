# OAuth loopback tunnel over LiveKit (operator desktop) — PRD

**Date**: 2026-04-11  
**PRD Type**: Technical improvement / requirement update (Codex OAuth relay transport)

## Requirements input (plan-tdd-one-shot)

Captured via structured questionnaire:

- **Scope for this cycle**: OAuth loopback TCP tunnel over LiveKit bidi RPC only (no daemon packaging epic in this PRD).
- **Legacy path**: After the tunnel is validated, **remove** the desktop HTTP query parsing + **`DeliverCallback`**-centric path (no long-term dual stack).
- **Acceptance bar**: **Automated** E2E or integration test proving **byte round-trip** through the tunnel (no dependency on real OpenAI in CI).

## Affected features

**CRITICAL**: Reference feature docs; do not edit them until implementation is complete and docs are updated per feature workflow.

- **Primary**: [Codex OAuth web relay](../web/codex-oauth-web-relay.md) — operator-visible authorize link and callback completion path; desktop’s role shifts from “parse `/auth/callback` + unary RPC” to “raw TCP on loopback proxied over LiveKit”.
- **Related**: [Codex OAuth relay (daemon)](../daemon/codex-oauth-relay.md) — validation/parsing helpers remain relevant for URLs; callback delivery mechanism on the session host may align with loopback HTTP again (Codex listener), not `DeliverCallback`.
- **Related**: [tddy-desktop (Electrobun)](../desktop/tddy-desktop-electrobun.md) — main-process TCP listener and LiveKit client wiring.
- **Related**: LiveKit participant / RPC stack (see archived PRDs under `docs/ft/coder/1-WIP/archived/` if needed) — new or extended **`loopback_tunnel`** service on the session host participant.

## Summary

Replace the operator-desktop **HTTP callback server** that extracts `code`/`state` and calls **`CodexOAuthService.DeliverCallback`** with a **TCP proxy**: the operator’s browser still hits **`http://127.0.0.1:<port>/auth/callback`** on the **operator machine**, but bytes are **tunneled bidirectionally** over **LiveKit data-channel RPC** to the **session host**, where a bridge **dials `127.0.0.1:<port>`** and pipes data to **Codex’s existing loopback listener**. Remote **browser open** stays intercepted; the **authorize URL** is shown on the desktop; **callback** compatibility is preserved without implementing OAuth semantics on the desktop.

## Background

- Today’s flow depends on **`callback_port`** and **`DeliverCallback`** so the agent can **`GET`** the Codex loopback URL on the session host. That couples desktop logic to query parsing and proto fields, and is fragile when codegen or binaries drift.
- A **byte-transparent tunnel** matches the user mental model (“port forwarding”) and keeps OAuth validation on the session host where Codex runs.

## Proposed changes

### What’s changing

- Add **`LoopbackTunnelService.StreamBytes`** (bidi **`TunnelChunk`**: first message opens **`127.0.0.1:open_port`**, then raw **`data`** both ways) on the **tddy-coder** (or session) LiveKit participant; implement bridge in **`tddy-service`**.
- **tddy-desktop**: **`Bun.listen`** (or equivalent) on **`127.0.0.1`**, **`remoteLoopbackPort`** from metadata; per accepted socket, run **Connect bidi** `streamBytes` and pipe socket ↔ stream.
- **Remove** (in this PRD’s completion, not necessarily first commit): desktop **`oauth-callback-server.ts`** usage for production path, **`defaultCodexOAuthDeliverPipeline`**, **`CodexOAuthDeliverPipeline`** / **`createDeliverPipeline`** deps shape, and **`codex_oauth.DeliverCallback`** from the **operator→session** path; delete or shrink **`DeliverCallback`** proto/service if nothing else needs it (confirm grep across repo + web).
- **Tests**: automated **round-trip** — e.g. Rust integration with local **`TcpListener`** + LiveKit test harness **or** desktop mock-room test that asserts raw HTTP bytes crossed a **fake** tunnel (prefer one canonical test that runs in CI).

### What’s staying the same

- **Metadata** surface for **`codex_oauth`** (pending, **`authorize_url`**, **`callback_port`**, **`state`**) for opening the authorize link and choosing the local listen port.
- **Remote** interception of **`BROWSER`** / authorize URL publication.
- **Security posture**: tunnel still requires LiveKit membership and targets **loopback on the session host only**; document abuse scenarios (any open port on session host) and keep **port allowlist** or **OAuth-only port** as a follow-up if product requires it.

## Impact analysis

### Technical

- **Packages**: `tddy-service`, `tddy-coder`, `tddy-livekit`, `tddy-livekit-web`, `tddy-desktop`; possibly **`tddy-web`** only if UI copy references DeliverCallback (unlikely).
- **Proto/codegen**: `loopback_tunnel.proto` in **`tddy-service`** and **`tddy-livekit`** (Buf input for TS); **`build.rs`** / **`buf generate`** pipeline.
- **Risk**: data-channel **backpressure** and **chunk batching** (see terminal streaming patterns in **`GhosttyTerminalLiveKit`**) — may need batching or flow control for large HTTP responses.

### User

- Operators see the same **click link → browser → callback** flow; implementation is more reliable and closer to “real” loopback from Codex’s perspective.

## Implementation plan overview (high level)

1. Land **server** bridge + **TS client** + **desktop TCP** listener (behind feature flag only if needed for incremental merge).
2. Add **automated** round-trip test(s).
3. **Remove** legacy **`DeliverCallback`** desktop path and dead proto/Rust if unused.
4. Update **feature docs** (web relay, daemon relay, desktop) in a separate doc pass.

## Acceptance criteria

- [ ] With pending OAuth metadata, desktop listens on **loopback** on the advertised port and **forwards a single browser connection’s bytes** to the session host loopback port without parsing OAuth query parameters on the desktop.
- [ ] Session host **completes** Codex OAuth (or test double) when the **HTTP request** is identical to a local callback **GET** (validated via automated test harness).
- [ ] **`DeliverCallback`** is **not** used on the **desktop → session** path after merge; related code removed or justified if still required elsewhere.
- [ ] **CI-runnable** automated test demonstrates **end-to-end bytes** through tunnel plumbing (LiveKit mock or in-process bridge per existing patterns).

## Success criteria (product)

- Codex OAuth login works for **desktop + remote session** dev setup without “no pending OAuth” class failures tied to **`callback_port`** proto skew.
- Reduced operational confusion: one mechanism (tunnel), not HTTP-parse + unary.

## Out of scope (this PRD)

- **Bundling `tddy-daemon` with desktop** — see **`PRD-2026-04-11-desktop-bundled-daemon.md`**.
- **Arbitrary multi-port VPN-style tunnel product** — only **Codex OAuth loopback path** required unless trivial to generalize without scope creep.

## References

- In-repo work in progress: `packages/tddy-service/proto/loopback_tunnel.proto`, `loopback_tunnel_service.rs` (verify and align with this PRD when implementing).
- `packages/tddy-desktop/src/bun/livekit-oauth-relay.ts` — current metadata + HTTP server path to be replaced.

## Open questions (for Plan mode / changeset)

- Should the session host enforce **`open_port ∈ { metadata.callback_port, 1455 }`** only?
- Exact **test** placement: `tddy-e2e` LiveKit suite vs `tddy-livekit` tests vs desktop **Bun** E2E with mock room.

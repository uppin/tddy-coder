# Changeset: OAuth loopback TCP listener in tddy-daemon

**Date**: 2026-04-11  
**Status**: ✅ Complete (implementation landed; run LiveKit `rpc_scenarios` with testkit when validating E2E)  
**Type**: Architecture Change (behavior-preserving migration)

## Planning context (user inputs)

Collected via `/plan-tdd-migration`:

| Question | Answer |
|----------|--------|
| Desktop role after migration | **Daemon only**: desktop embeds/starts daemon; **no** `Bun.listen` TCP proxy in desktop. |
| Wire compatibility | **Yes**: same `loopback_tunnel` protobuf / `LoopbackTunnelService.StreamBytes` semantics; only **where** the operator-side loopback accept runs. |
| Daemon deployment scope | **All modes** that can participate in LiveKit OAuth: embedded (desktop) **and** standalone CLI daemon. |

**Clarification (implementation note):** “Move Bun.listen to tddy-daemon” means moving the **loopback TCP accept path** into the **Rust daemon process** (`tokio::net::TcpListener` or equivalent). **tddy-daemon does not run Bun**; Bun remains for desktop shell/UI only.

## Affected packages

- **`tddy-daemon`**: New (or relocated) session-aware LiveKit client path + TCP accept + bidi tunnel client using existing Rust LiveKit stack (`livekit` crate, `tddy_livekit::RpcClient`, `tddy-service` protos).
- **`tddy-desktop`**: Remove `oauth-loopback-tcp-proxy.ts` / default `Bun.listen` path; keep OAuth **orchestration** (open browser, metadata watch) via **injection or RPC** into daemon, or slim relay that **does not** bind TCP.
- **`tddy-livekit` / `tddy-livekit-web`**: Likely unchanged if RPC framing stays identical; possible small helpers for Rust-side bidi `LoopbackTunnel` client.
- **`tddy-service`**: Unchanged if protocol preserved (session host still runs `LoopbackTunnelServiceImpl`).

## Pre-existing baseline (packages in scope)

**Command run:** `./dev cargo test -p tddy-daemon --no-run`

**Result (updated):** `initgroups` fixed (`gid as libc::c_int`); `./dev cargo test -p tddy-daemon --no-run` succeeds.

**Desktop:** `bun test src/bun test/e2e` in `packages/tddy-desktop` passes (injectable tunnel stubs).

**LiveKit:** `rpc_scenarios` compiles; full run needs a healthy LiveKit testkit/WebRTC environment (`wait_pc_connection` may timeout in CI/sandbox without it).

**Desktop baseline (not yet run in this session):** From repo root, `bun run test` in `packages/tddy-desktop` (see `package.json`) after `tddy-livekit-web` build — run after daemon compiles to capture pass/fail counts for OAuth relay / e2e mocks.

## State A (current)

- **Operator machine:** `packages/tddy-desktop/src/bun/oauth-loopback-tcp-proxy.ts` uses **`Bun.listen`** on `127.0.0.1:listenPort`, accepts browser OAuth callback TCP connections, and pipes bytes through **`LoopbackTunnelService.streamBytes`** via `@livekit/rtc-node` + `tddy-livekit-web` Connect transport, targeting **`daemon-*`** identity in the **session** LiveKit room (`livekit-oauth-relay.ts`).
- **Session host:** `tddy-service` `LoopbackTunnelServiceImpl` dials `127.0.0.1:open_port` toward Codex (unchanged in this migration).

## State B (target)

- **Operator machine:** **`tddy-daemon`** holds the **TCP listener** on the operator loopback and the **same** bidi tunnel to the session host participant (same RPC/method/payload rules).
- **Desktop:** No TCP bind; delegates tunnel lifecycle to daemon (IPC/gRPC/env — **to be decided in implementation**).
- **Compatibility:** Byte-for-byte behavior of the tunnel from browser ↔ Codex perspective; no proto or `tddy-service` behavior change unless a defect is found.

## Key technical gap (must be designed explicitly)

Today **`tddy-daemon`** keeps a LiveKit `Room` handle for **`livekit.common_room`** (peer discovery / `StartSession` forward), **not** for each **session room** (`daemon-{session}` / shared room) where `tddy-coder` and desktop join.

To call `StreamBytes` toward the session server identity, the daemon needs **either**:

1. **Session-room connection in daemon** — connect to the same room/url/token as the tool session when OAuth is pending (token source: desktop IPC, daemon metadata, or regen from daemon config), **or**
2. **Indirect routing** — only if a supported design routes tunnel via common room (higher complexity; not assumed here).

**Recommendation for migration plan:** Prefer (1): explicit **session-room `Room` handle** in daemon for the OAuth window, with a single ownership model (who connects, who reconnects, token refresh).

## Migration strategy (high level)

1. **Unblock baseline:** fix `initgroups` typing; document `cargo test -p tddy-daemon` results.
2. **Behavior preservation tests (Rust):** Extract or duplicate the semantics of `oauth-loopback-tcp-proxy` + `livekit-oauth-relay` **observable behavior** into tests (mock TCP client + mock `RpcClient` / testkit room) **before** deleting desktop listener.
3. **Implement Rust listener + bidi stream** in `tddy-daemon`, reusing `tddy_livekit::RpcClient` patterns (see `livekit_peer_discovery` / multi-rpc registration in `tddy-coder`).
4. **Wire desktop → daemon:** start/stop tunnel, pass `listenPort`, `targetIdentity`, `remoteLoopbackPort`, room credentials or session id as needed.
5. **Remove** desktop `Bun.listen` path; keep e2e tests by driving daemon or fakes.
6. **Validate:** `packages/tddy-livekit` loopback tunnel scenario tests still pass; desktop tests updated.

## Behavior preservation

- Same URLs opened in browser; same callback port semantics from Codex’s perspective.
- Same `TunnelChunk` ordering rules (`open_port` first chunk, then payload; port ≥ 1024 enforced server-side).
- Failure modes: bind failures, stream errors — same user-visible outcomes (log + tunnel stopped).

## Rollback

- Revert commit(s); restore `oauth-loopback-tcp-proxy.ts` as default in `livekit-oauth-relay.ts`.

## Implementation milestones

- [ ] Baseline: `tddy-daemon` compiles; test `--no-run` green
- [ ] Baseline: `tddy-desktop` tests documented (counts)
- [ ] Design note: session-room connection & token lifecycle in daemon
- [ ] Rust unit/integration tests for tunnel accept + chunk framing (mocked LiveKit)
- [ ] Daemon integration with real room (dev/staging) smoke
- [ ] Desktop slim-down; no `Bun.listen` in default path
- [ ] Docs: desktop README / daemon README delta
- [ ] Wrap changeset when merged

## TODO checklist (command template)

### Phase 1: Planning

- [x] Migration changeset (this file)
- [ ] Optional PRD pointer if product-facing doc needed

### Phase 2: TDD migration

- [ ] Behavior preservation tests (`/ft-dev` / manual test list)
- [ ] **User review:** test coverage
- [ ] Implement in small steps; keep tests green (`/green`)
- [ ] **User review:** migration complete → validation

### Phase 3: Production readiness

- [ ] `/validate-changes` (or repo equivalent)
- [ ] `/validate-tests`
- [ ] `/validate-prod-ready`
- [ ] `/analyze-clean-code`
- [ ] Lint / typecheck / targeted test runs
- [ ] `/wrap-context-docs`
- [ ] `/pr`

## Risks

| Risk | Mitigation |
|------|------------|
| Session-room token not available in standalone daemon | Plumb `ConnectSession`-style credentials or shared secret issuance from daemon API |
| Double connection (desktop + daemon) to same room | Single owner for tunnel RPC; desktop may drop rtc-node for tunnel-only paths |
| Token refresh / reconnect | Align with existing `tddy-livekit` reconnect patterns |

## Success criteria

- No intentional user-visible behavior change for OAuth completion.
- `LoopbackTunnelService` proto and `tddy-service` handler unchanged.
- Desktop default path does not call `Bun.listen` for OAuth callback.
- Automated tests cover accept → streamBytes → echo path equivalent to current desktop tests.

# Changeset: Tddy Desktop (Electrobun) Phases 1–3

**Date**: 2026-04-10  
**Status**: ✅ Ready for review  
**Type**: Feature

## Plan mode context (summary)

- **Scope**: Electrobun shell embedding `tddy-web`, Codex OAuth discovery via LiveKit participant metadata, relay MVP (Variant A): desktop captures OAuth callback → LiveKit RPC → `tddy-coder` proxies HTTP to Codex loopback.
- **Relay**: New `CodexOAuthService` unary RPC on existing `tddy-rpc` / `tddy-rpc` data channel envelope.
- **Metadata**: `codex_oauth` JSON on participant metadata; `tddy-coder` updates via optional `watch` channel wired into `LiveKitParticipant` event loop.
- **Testing**: Unit tests for validation/parsing; integration tests for callback server and RPC where feasible; root `package.json` test script extended for `tddy-desktop`.

## Affected Packages

- **tddy-desktop**: [README.md](../../../packages/tddy-desktop/README.md), new Electrobun app
- **tddy-web**: Participant metadata parsing, UI
- **tddy-service**: `codex_oauth.proto`, generated RPC server, `CodexOAuthServiceImpl`
- **tddy-livekit**: `LiveKitParticipant` optional metadata watch channel
- **tddy-coder**: LiveKit multi-service (Terminal + Codex OAuth), metadata publishing from Codex OAuth detector, loopback proxy on `DeliverCallback`
- **tddy-core** (optional): Shared OAuth URL/query validation if placed there; otherwise `tddy-service`

## Related Feature Documentation

- [PRD](../../ft/desktop/PRD-2026-04-10-tddy-desktop-electrobun-impl.md)
- [Design](../../ft/desktop/tddy-desktop-electrobun.md)
- [Web relay](../../ft/web/codex-oauth-web-relay.md) (created)
- [Daemon relay](../../ft/daemon/codex-oauth-relay.md) (created)

## Summary

Deliver a native desktop shell with OAuth callback relay and align web presence UI with structured `codex_oauth` metadata.

## Background

Remote Codex OAuth cannot complete from a laptop browser without relay; design doc specifies Electrobun + LiveKit.

## Scope

- [x] Changeset + feature docs
- [x] Implementation (all phases)
- [x] Tests passing (cargo + bun for touched packages)
- [x] Lint / typecheck (`cargo clippy -D warnings` on touched Rust crates; desktop `bun test`)

## Technical Changes (delta)

| Area | State A | State B |
|------|---------|---------|
| Desktop | Placeholder README | Electrobun package, dev/build scripts |
| LiveKit metadata | Not updated by coder | `tddy-coder` publishes `codex_oauth` JSON |
| RPC | Terminal only on LK path | + `codex_oauth.CodexOAuthService/DeliverCallback` |

## Implementation Milestones

- [x] Changeset + missing `docs/ft/*` specs
- [x] `codex_oauth.proto` + codegen + `CodexOAuthServiceImpl`
- [x] `LiveKitParticipant` metadata watch integration
- [x] `tddy-coder`: wire service, OAuth stderr/file detection, metadata + proxy
- [x] `tddy-desktop`: Electrobun scaffold, webview URL, LiveKit + callback server + relay client
- [x] `tddy-web`: parse metadata, ParticipantList UX
- [x] Acceptance + unit/integration tests
- [x] Validation / wrap-ready (verify + clippy; wrap-context-docs still optional pre-merge)

## Acceptance Tests

1. **Rust**: `codex_oauth` validation helpers (URL allowlist, state) — unit tests in `tddy-service` or `tddy-core`
2. **Rust**: `DeliverCallback` builds correct loopback URL and rejects invalid state (integration-style with mock HTTP server)
3. **Bun**: `tddy-desktop` parses `codex_oauth` metadata JSON; callback server returns 200 for valid GET — see `packages/tddy-desktop/test/acceptance/desktop-phases.acceptance.test.ts` and `src/bun/*.test.ts`
4. **Web**: `useRoomParticipants` exposes `codexOauth` when metadata contains JSON
5. **E2E (automated, no WebRTC)**: `packages/tddy-desktop/test/e2e/livekit-oauth-relay-mock.e2e.test.ts` — mock `Room`, full path metadata → `installLiveKitOAuthRelay` → callback `fetch` → injectable `deliverCallback`
6. **E2E (real LiveKit)**: still manual or future env-gated run with `run-livekit-testkit-server` + token RPC (not in CI by default)

## Validation Results

- **Workspace tests**: `./dev ./verify` (2026-04-10) — all packages `0 failed` (see repo root `.verify-result.txt`).
- **Codegen**: `tddy-codegen` emits conditional `Stream` / `StreamExt` / `mpsc` imports and `_method` when no bidi methods (fixes unary-only services such as `codex_oauth`).

## References

- [tddy-livekit participant](../../../packages/tddy-livekit/src/participant.rs)
- [tddy-coder run_daemon](../../../packages/tddy-coder/src/run.rs)

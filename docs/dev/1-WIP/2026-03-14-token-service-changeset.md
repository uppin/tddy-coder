# Changeset: TokenService RPC Exposure

**Date:** 2026-03-14
**PRD:** [PRD-2026-03-14-token-service.md](../../ft/coder/1-WIP/PRD-2026-03-14-token-service.md)

## Summary

Exposed a new `TokenService` via LiveKit RPC that allows callers to generate and refresh LiveKit access tokens without holding API credentials. The service delegates to `TokenGenerator` through a `TokenProvider` trait.

## Implementation

### Packages Changed

| Package | Changes |
|---------|---------|
| tddy-livekit | Added `generate_for(room, identity)` and `ttl()` to `TokenGenerator` |
| tddy-rpc | Added `MultiRpcService` and `ServiceEntry` for multiplexing multiple RPC services |
| tddy-service | New `token.proto`, `TokenServiceImpl`, `TokenProvider` trait |
| tddy-coder | Wired `LiveKitTokenProvider` + `TokenService` into LiveKit participant when API key/secret configured |
| tddy-e2e | New `token_service_livekit.rs` e2e test |

### Key Design Decisions

- **TokenProvider trait**: Decouples `tddy-service` from `tddy-livekit` (avoids pulling WebRTC into service package). Implementation lives in `tddy-coder`.
- **MultiRpcService**: Combines TerminalService + TokenService on a single LiveKit participant when credentials are available.
- **gRPC exposure**: Deferred; TokenService is currently LiveKit RPC only.

### Files Added/Modified

- `packages/tddy-livekit/src/token.rs` — `generate_for`, `ttl`, refactored `generate()` to delegate
- `packages/tddy-rpc/src/bridge.rs` — `MultiRpcService`, `ServiceEntry`
- `packages/tddy-service/proto/token.proto` — New
- `packages/tddy-service/build.rs` — Token proto compilation
- `packages/tddy-service/src/token_service.rs` — New
- `packages/tddy-service/src/lib.rs` — Module, exports, proto include
- `packages/tddy-service/src/integration_tests.rs` — TokenService acceptance tests
- `packages/tddy-coder/src/run.rs` — `LiveKitTokenProvider`, MultiRpcService wiring
- `packages/tddy-coder/Cargo.toml` — Added tddy-rpc
- `packages/tddy-e2e/tests/token_service_livekit.rs` — New
- `packages/tddy-e2e/Cargo.toml` — Added tddy-rpc

## Validation

- `cargo test -p tddy-service` — 3 token_service_acceptance tests pass
- `cargo test` — All tests pass (livekit e2e requires `--features livekit`; may crash on some macOS/WebRTC setups)
- `cargo clippy -- -D warnings` — Passes

# Changeset: Dual-Transport Service Codegen

**Status:** Complete
**PRD:** [docs/ft/coder/1-WIP/PRD-2026-03-13-dual-transport-codegen.md](../../ft/coder/1-WIP/PRD-2026-03-13-dual-transport-codegen.md)

## Summary

- Create `tddy-rpc` (generic RPC types + dispatch)
- Rename `tddy-livekit-codegen` -> `tddy-codegen`
- Rename `tddy-grpc` -> `tddy-service`, move echo/terminal from tddy-livekit
- Slim `tddy-livekit` to thin LiveKit transport adapter
- Generate RpcService server structs + tonic adapter

## Milestones

- [x] M1: Create tddy-rpc crate
- [x] M2: Rename tddy-livekit-codegen to tddy-codegen
- [x] M3: Rename tddy-grpc to tddy-service + move service code
- [x] M4: Slim tddy-livekit to thin adapter
- [x] M5: TddyServiceGenerator configurable
- [x] M6: Generate server struct + per-method handlers
- [x] M7: Migrate service impls to generated server structs
- [x] M8: Generate tonic adapter (feature-gated)

## Validation (2026-03-13)

- **tddy-rpc, tddy-codegen, tddy-service**: Tests pass, clippy clean
- **tddy-e2e, tddy-core**: Tests pass (livekit feature optional for webrtc-free CI)
- **tddy-livekit**: Build fails on macOS/Nix due to webrtc-sys `uuid_string_t` (pre-existing env issue, not plan-related)
- **terminal_service_acceptance**: Moved to tddy-e2e; `packages/tddy-livekit/proto/terminal.proto` removed (unused)

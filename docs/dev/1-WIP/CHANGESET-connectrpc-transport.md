# Changeset: ConnectRPC Transport Module

**Date**: 2026-03-14
**Status**: Implemented (unary + server/client/bidi streaming)
**Related PRD**: [PRD-2026-03-14-connectrpc-transport](../../ft/coder/1-WIP/PRD-2026-03-14-connectrpc-transport.md)

## Summary

Add `tddy-connectrpc` crate implementing Connect protocol HTTP transport for `tddy-rpc` services. Mount at `/rpc` on tddy-coder's web-port. Companion `tddy-rust-typescript-tests` bun package for integration tests.

## Scope

- **New crate**: `packages/tddy-connectrpc` — axum router, protocol handling, envelope framing
- **Integration**: `packages/tddy-coder` — mount ConnectRPC router in web server
- **Tests**: `packages/tddy-rust-typescript-tests` — bun-based TS client integration tests

## Implementation Phases

1. **Milestone 1**: Unary RPC with protobuf binary encoding — DONE
2. **Milestone 2**: Error responses in Connect JSON format — DONE
3. **Milestone 3**: Server streaming with Connect envelope framing — DONE
4. **Milestone 4**: Client streaming and bidi streaming — DONE
5. **Milestone 5**: Mount ConnectRPC router at `/rpc` in tddy-coder — DONE
6. **(Future)**: JSON encoding via pbjson integration

## Technical Decisions

- Protobuf-binary-first (zero-cost passthrough through RpcBridge)
- Reuse RpcBridge for dispatch (same path as LiveKit)
- Own envelope implementation (5-byte framing per connectrpc-axum pattern)
- Extend tddy_rpc::Code with to_connect_str() and to_http_status()

## Validation Results

### Build Validation

| Package | Status | Notes |
|---------|--------|-------|
| tddy-connectrpc | Pass | Built successfully |
| tddy-coder | Pass | Built successfully |
| tddy-service | Pass | Built successfully |
| tddy-rpc | Pass | Built successfully |
| tddy-web | Pass | Bundled successfully |

### Test Validation

| Package | Status | Notes |
|---------|--------|-------|
| tddy-connectrpc | Pass | 5/5 acceptance tests (unary, error, server stream, client stream, bidi) |
| tddy-coder | Pass | web_bundle_acceptance passes |
| tddy-rust-typescript-tests | Pass | 1 skip (CONNECTRPC_TEST_SKIP=1); test:live requires live server |
| tddy-livekit rpc_scenarios | Env | Docker port 7880 in use; use LIVEKIT_TESTKIT_WS_URL with ./run-livekit-testkit-server |

### Test Quality (validate-tests)

- **tddy-connectrpc/tests/acceptance.rs**: 5 tests, all with assertions; no .skip/.only; deterministic
- **tddy-rust-typescript-tests**: 1 test with skipIf(CONNECTRPC_TEST_SKIP) for live-server requirement; intentional

### Production Readiness (validate-prod-ready)

- No mock/fake code in production paths
- No development fallbacks
- No TODO/FIXME in tddy-connectrpc
- Envelope module is public for test use; acceptable

### Code Quality (analyze-clean-code)

- router.rs handle_rpc: ~135 lines; could extract parse_body/build_response helpers in future
- envelope.rs, protocol.rs, error.rs: small, focused modules
- No magic values; constants in protocol.rs

### Production Code

- No fallbacks added
- No test-only branches in production code
- Envelope parsing handles streaming request bodies; protocol.is_streaming() gates behavior

## Reference

- [connectrpc-axum analysis](./connectrpc-axum-analysis.md)
- [connect-es protocol](tmp/connect-es/) (wire format reference)

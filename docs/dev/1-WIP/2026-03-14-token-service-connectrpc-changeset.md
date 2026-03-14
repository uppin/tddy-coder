# Changeset: TokenService via Connect-RPC

**Date:** 2026-03-14
**PRD:** [PRD-2026-03-14-client-side-token-auth.md](../../ft/web/1-WIP/PRD-2026-03-14-client-side-token-auth.md) (updated for server-side approach)

## Summary

Exposed `TokenService` via Connect-RPC on the web server. tddy-web now fetches tokens from the server (identity + room) instead of requiring pre-generated JWTs. Added `getToken` prop to `GhosttyTerminalLiveKit` for token refresh 1 minute before expiry.

## Implementation

### Packages Changed

| Package | Changes |
|---------|---------|
| tddy-connectrpc | New acceptance test `token_service_generate_token_returns_200_with_token_and_ttl` |
| tddy-coder | Wire `TokenService` into Connect-RPC router via `MultiRpcService` (run_daemon, run_full_workflow_tui) when API key/secret present |
| tddy-web | New form (identity, url, room); Connect-RPC client; `getToken` prop on GhosttyTerminalLiveKit; buf codegen for token_pb.ts |

### Key Design Decisions

- **baseUrl `/rpc`**: Connect-Web transport uses `window.location.origin + '/rpc'` to match server route `/rpc/{service}/{method}`.
- **useBinaryFormat: true**: Client sends/receives protobuf binary (server expects `application/proto`).
- **GenerateToken for initial, RefreshToken for refresh**: App fetches initial token via `GenerateToken`, passes `getToken` that calls `RefreshToken` for periodic refresh.
- **Reconnect on Disconnected**: When `getToken` provided, `RoomEvent.Disconnected` triggers reconnect with `latestTokenRef.current`.
- **Backward compat**: `token` prop still works; Storybook and e2e tests pass pre-generated tokens via URL params.

### Files Added/Modified

- `packages/tddy-connectrpc/tests/acceptance.rs` — TokenService Connect-RPC test
- `packages/tddy-coder/src/run.rs` — MultiRpcService with Echo + Token when has_key_secret
- `packages/tddy-web/src/gen/token_pb.ts` — Generated from token.proto
- `packages/tddy-web/src/index.tsx` — New form, Connect-RPC client, getToken
- `packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx` — getToken prop, refresh timer
- `packages/tddy-web/package.json` — @connectrpc/connect-web, @storybook/react, buf deps
- `packages/tddy-web/buf.yaml`, `buf.gen.yaml` — Proto codegen
- `packages/tddy-web/cypress/component/App.cy.tsx` — Token fetch acceptance test (binary protobuf mock)
- `packages/tddy-web/cypress/e2e/app-connect-flow.cy.ts` — E2E test for Connect flow (requires LIVEKIT_TESTKIT_WS_URL)
- `packages/tddy-web/cypress.config.ts` — startTddyCoderForConnectFlow, stopTddyCoderForConnectFlow tasks

## Validation

- `cargo test` — All Rust tests pass
- `bun run cypress:component` — App.cy.tsx, Button, GhosttyTerminal pass
- `bun run cypress:e2e` — Storybook stories pass
- `cargo clippy -- -D warnings` — Passes
- `bunx tsc --noEmit` — Passes

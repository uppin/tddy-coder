# PRD: Server-Side LiveKit Token Authentication via Connect-RPC

**Status:** ✅ Complete (documentation wrapped)

## Summary

Replace the pre-generated JWT token input in tddy-web with server-side token generation via Connect-RPC. The user provides Identity and Room name; the tddy-coder server (with API key/secret) generates tokens on demand. Add periodic token refresh to maintain long-running sessions.

## Background

Previously, tddy-web required users to manually generate a LiveKit JWT token using the `lk` CLI tool and paste it into the connection form. An alternative approach (client-side JWT via `jose`) was considered but rejected in favor of server-side generation: the API secret must never reach the browser.

## Affected Features

- [web-terminal.md](../../web-terminal.md) — Connection flow, authentication UX

## Proposed Changes

### What's Changing

1. **Connection form**: Replace the "Token" field with "Identity" and "Room name". LiveKit URL remains user-provided.
2. **Token generation**: Tokens are fetched from the server via Connect-RPC `TokenService.GenerateToken(room, identity)`. The server holds API key/secret and generates JWTs.
3. **URL parameter security**: Only persist LiveKit URL, Identity, and Room name in query params. No secrets in the URL.
4. **Token refresh**: `GhosttyTerminalLiveKit` accepts an optional `getToken` callback. When provided, it calls `TokenService.RefreshToken` 1 minute before expiry and stores the fresh token for reconnection.

### What's Staying the Same

- `GhosttyTerminalLiveKit` component API (still accepts `url` and `token` props for backward compat)
- LiveKit room connection and RPC transport logic
- Cypress e2e test infrastructure (tests generate tokens server-side via `livekit-server-sdk`)
- Storybook stories (pass pre-generated token via URL params)

## Technical Approach

- Connect-RPC HTTP transport: `createConnectTransport({ baseUrl: origin + '/rpc' })`
- `TokenService.GenerateToken` and `TokenService.RefreshToken` return `{ token, ttlSeconds }`
- `GhosttyTerminalLiveKit` when given `getToken`: calls it on mount, schedules refresh at `(ttlSeconds - 60) * 1000` ms
- API key/secret are configured on tddy-coder (`--livekit-api-key`, `--livekit-api-secret`). When absent, TokenService is not exposed via Connect-RPC.

## Acceptance Criteria

1. User can connect by entering LiveKit URL, Identity, and Room name (tddy-coder must be running with API key/secret)
2. No secrets are stored in URL query parameters
3. Token is fetched from server via Connect-RPC on connect
4. Token is automatically refreshed ~1 minute before expiration via `getToken` callback
5. Token refresh does not interrupt the active terminal session
6. Existing Cypress e2e tests continue to pass (pre-generated tokens via Storybook URL params)
7. When tddy-coder runs with `--livekit-token` only (no key/secret), TokenService is not available; user must provide token manually

---

## Documentation wrap

Merged into [web-terminal.md](../../web-terminal.md) and [Web changelog](../../changelog.md) on 2026-03-21. This file is archived under `1-WIP/archived/`.

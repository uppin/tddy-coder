# PRD: LiveKit Token Generation from API Key/Secret

**Status:** WIP
**Date:** 2026-03-14

## Summary

Add support for generating LiveKit access tokens locally from API key and secret, as an alternative to providing a pre-generated `--livekit-token`. When using key/secret, tokens are generated on the fly and automatically refreshed by reconnecting 1 minute before expiry.

## Background

Currently, the daemon and TUI modes require a pre-generated JWT token via `--livekit-token`. This has operational friction: users must generate tokens externally, manage expiry, and restart the process when tokens expire. Since the `livekit-api` crate (already used in `tddy-livekit-testkit`) provides `AccessToken::with_api_key()`, we can generate tokens directly from API credentials — eliminating the external token generation step and enabling automatic refresh.

## Proposed Changes

### New CLI Arguments

- `--livekit-api-key <KEY>` — LiveKit API key (also loadable from `LIVEKIT_API_KEY` env var)
- `--livekit-api-secret <SECRET>` — LiveKit API secret (also loadable from `LIVEKIT_API_SECRET` env var)

**Mutual exclusivity:** `--livekit-token` and `--livekit-api-key`/`--livekit-api-secret` are mutually exclusive. Providing both is an error. One of the two must be provided (alongside `--livekit-url`, `--livekit-room`, `--livekit-identity`).

### Token Generation

When key/secret are provided:
1. Generate a JWT with `AccessToken::with_api_key(key, secret)`, setting identity, room (with `room_join: true`), and a 2-minute TTL
2. Use the generated token for `Room::connect()`

### Token Refresh (Reconnection)

Since the Rust SDK's `Room::connect()` takes a literal token string with no built-in refresh mechanism, token refresh requires reconnection:

1. Track the token's expiry time (known from the TTL at generation time)
2. 1 minute before expiry, generate a fresh token
3. Close the current Room connection and reconnect with the new token
4. The reconnection loop continues for the lifetime of the process

Default TTL is 2 minutes, meaning reconnection happens every ~1 minute.

### Package Changes

**`tddy-livekit`** — Add `livekit-api` dependency, new `token` module:
- `TokenGenerator` struct holding API key, secret, room, identity, TTL
- `generate() -> Result<String>` method producing a JWT
- `time_until_refresh() -> Duration` returning TTL minus 1 minute
- Reconnecting participant wrapper that handles the connect → run → refresh cycle

**`tddy-coder`** — CLI argument additions and validation:
- New `--livekit-api-key` and `--livekit-api-secret` args on `CoderArgs` and `DemoArgs`
- Mutual exclusivity validation with `--livekit-token`
- Pass token generator to the LiveKit connection code paths (daemon mode + TUI mode)

## Affected Features

- [PRD-2026-03-11-livekit-participant.md](PRD-2026-03-11-livekit-participant.md) — Extends LiveKit connectivity with token generation and refresh
- [grpc-remote-control.md](../grpc-remote-control.md) — Daemon startup flow gains token refresh reconnection loop

## Technical Constraints

- The `livekit` Rust SDK `Room::connect()` takes a literal `&str` token — no `TokenSource` abstraction exists in the Rust SDK (only in JS/Swift frontend SDKs)
- Token refresh requires full disconnect/reconnect cycle — brief connection gap during refresh
- Service reuse across reconnections via `connect_with_bridge` — same `Arc<RpcBridge<S>>` shared across cycles
- The `livekit-api` crate supports `AccessToken::new()` which reads from `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` env vars natively

## Dependencies

| Crate | Version | Purpose | Status |
|-------|---------|---------|--------|
| `livekit-api` | 0.4 | Token generation (`AccessToken`) | New dep for `tddy-livekit` |

## Success Criteria

1. `--livekit-api-key` and `--livekit-api-secret` (or env vars) generate a valid token and connect to LiveKit
2. Providing both `--livekit-token` and `--livekit-api-key` produces a clear error
3. Token is automatically refreshed by reconnecting 1 minute before expiry
4. Reconnection is seamless — the participant rejoins the room with a fresh token
5. Existing `--livekit-token` flow continues to work unchanged (no refresh)
6. Both daemon mode and TUI mode support the new token generation

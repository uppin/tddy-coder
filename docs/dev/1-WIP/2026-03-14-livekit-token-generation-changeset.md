# Changeset: LiveKit Token Generation from API Key/Secret

**Date:** 2026-03-14
**PRD:** [PRD-2026-03-14-livekit-token-generation.md](../../ft/coder/1-WIP/PRD-2026-03-14-livekit-token-generation.md)

## Summary

Implemented LiveKit token generation from API key/secret with automatic refresh. Users can now use `--livekit-api-key` and `--livekit-api-secret` (or env vars `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`) instead of pre-generating a token.

## Implementation

### tddy-livekit

- **New `token` module** (`src/token.rs`): `TokenGenerator` with `generate()` and `time_until_refresh()` (TTL minus 60s)
- **`connect_with_bridge`**: Accepts pre-built `Arc<RpcBridge<S>>` for service reuse across reconnections
- **`run_with_reconnect`**: Loop that generates token, connects, runs event loop, sleeps until TTL-60s, then reconnects. Exits on room disconnect or shutdown signal.
- **Dependency**: Added `livekit-api = "0.4"`

### tddy-coder

- **CLI args**: `--livekit-api-key`, `--livekit-api-secret` on both `CoderArgs` and `DemoArgs` (with `env = "LIVEKIT_API_KEY"` / `env = "LIVEKIT_API_SECRET"`)
- **Validation**: `validate_livekit_args()` — mutual exclusivity of token vs key/secret, completeness check
- **Connection paths**: Daemon and TUI modes branch on `has_key_secret` — use `TokenGenerator` + `run_with_reconnect` when set, else existing `connect` + `run`
- **Clap**: Added `env` feature for env var support

### Tests

- **tddy-livekit**: Unit tests for `TokenGenerator` (generate, time_until_refresh, saturates when TTL short)
- **tddy-e2e**: `server_connects_via_token_generator` (requires livekit feature; skipped without)
- **tddy-coder**: `livekit_token_and_api_key_mutually_exclusive` — verifies mutual exclusivity error

## Milestones

- [x] TokenGenerator in tddy-livekit
- [x] connect_with_bridge + run_with_reconnect
- [x] CLI args and validation
- [x] Daemon mode token generation path
- [x] TUI mode token generation path
- [x] Unit and integration tests

## Validation Results

- **Build**: `cargo build -p tddy-coder` passes
- **Tests**: `cargo test -p tddy-coder` passes (41 tests). `cargo test -p tddy-livekit` passes (7 tests). rpc_scenarios fails due to Docker port conflict (env-specific).
- **Lint**: `cargo fmt`, `cargo clippy -- -D warnings` pass
- **Security**: API key/secret passed via CLI or env; no hardcoding. Token generation uses livekit-api crate.

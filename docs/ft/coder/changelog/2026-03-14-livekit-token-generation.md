# 2026-03-14 — LiveKit Token Generation

- **CLI args**: `--livekit-api-key` and `--livekit-api-secret` (or `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET` env vars) generate tokens locally instead of requiring pre-generated `--livekit-token`.
- **Mutual exclusivity**: Providing both token and key/secret is an error; one must be chosen.
- **Token refresh**: When using key/secret, tokens auto-refresh by reconnecting 1 minute before expiry. Reconnection loop runs for process lifetime.
- **Modes**: Both daemon and TUI support token generation when key/secret are set.
- **Packages**: tddy-livekit (TokenGenerator, connect_with_bridge, run_with_reconnect), tddy-coder (CLI args, validation, connection paths), tddy-e2e (server_connects_via_token_generator test).

# 2026-03-14 — LiveKit Token Generation

**Type:** Feature

TokenGenerator in token.rs (generate, time_until_refresh). connect_with_bridge accepts Arc<RpcBridge<S>> for service reuse. run_with_reconnect: token refresh loop (TTL minus 60s), reconnects before expiry. livekit-api 0.4 dependency. (tddy-livekit)

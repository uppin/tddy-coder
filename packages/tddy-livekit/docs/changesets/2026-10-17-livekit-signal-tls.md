# 2026-10-17 — Signal client connects to `wss://` servers

`livekit` is built with `rustls-tls-native-roots`, so the signal websocket can open a `wss://`
URL. Before this, the SDK's `tokio-tungstenite` had no TLS backend and every `wss://` connect
failed with `URL error: TLS support not compiled in` — a daemon pointed at a LiveKit server behind
TLS retried the common room forever and never discovered a peer.

- The feature is declared once, here; Cargo's feature unification gives every workspace crate that
  uses `livekit` the same backend.
- Certificates are verified against the OS trust store (native roots).
- Every shipped binary compiles rustls with the `ring` provider only, so no process-level
  `CryptoProvider` has to be installed.
- `tests/signal_tls.rs` pins that a `wss://` connect attempts the TLS handshake rather than being
  refused for want of TLS.

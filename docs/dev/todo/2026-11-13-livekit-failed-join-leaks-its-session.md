# 2026-11-13 — livekit SDK: a join that fails at ICE leaks its session

**Category:** Upstream defect, worked around
**Source:** incident 2026-11-13 (daemon at its 1,024-descriptor limit);
`docs/dev/1-WIP/2026-11-13-webrtc-runtime-pinned-for-the-process.md`

In livekit 0.7.47 — and unchanged on the SDK's `main` at 0.11.0 — `EngineInner::connect`'s
`try_connect` (`livekit/src/rtc_engine/mod.rs`) does

```rust
let (session, join_response, session_events) = RtcSession::connect(..).await?;
session.wait_pc_connection().await?;
```

and when the ICE wait fails the `RtcSession` is dropped without `close()`. `RtcSession` has no
`Drop`; the four tasks it spawned (`signal_task`, `rtc_session_task`, `data_channel_task`,
`dt_sender_task`) each hold an `Arc<SessionInner>` and wait on a `close_rx` that is never signalled,
so the session's peer connections — and, through the native `PeerConnection`'s
`shared_ptr<RtcRuntime>`, the libwebrtc threads they were created on — live for the rest of the
process. `full_reconnect` drops a failed `new_session` the same way.

`tddy_livekit::pin_webrtc_runtime()` stops the *runtime* from being recreated, which is what
turned this into a descriptor and thread leak. The session objects and their tasks still leak,
one set per failed join: memory only, slowly, but unbounded.

## What remains

- Fix upstream: close the session on a failed `wait_pc_connection` (both in `try_connect` and in
  `full_reconnect`), or give `RtcSession` a `Drop` that signals `close_tx`. File it against
  `livekit/rust-sdks` with `packages/tddy-livekit/tests/webrtc_runtime_bounded.rs` as the
  reproduction (relay-only ICE with no TURN server fails every join at `wait_pc_connection timed
  out`).
- Until it lands, a `[patch.crates-io]` on a fork is the only way to stop the memory growth; adding
  one is a dependency decision for the developer, not taken here.
- Deferred because the descriptor and thread growth — the part that took the daemon down — is
  closed by the pin, and the remaining growth is memory at a few kilobytes per failed join.

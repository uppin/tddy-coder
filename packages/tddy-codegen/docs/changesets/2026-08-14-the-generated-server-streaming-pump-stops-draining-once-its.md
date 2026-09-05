# 2026-08-14 — the generated server-streaming pump stops draining once its consumer is gone

**Type:** Fix

it was `let _ = tx.send(...).await;`, discarding the send error and pulling a handler's stream into a void forever after the peer disconnected; that kept the handler's receiver alive, so the handler's sender was never closed, so its own "has my subscriber gone?" check could never fire and its task ran until the process exited. Fixed at all three emit sites (server-streaming, client+server-streaming, bidi). Latent since the pump was written and shared by every streaming RPC: `StreamHostStats` got away with it by emitting every 5 s regardless, and it took an idle-by-design feed doing remote HTTP fan-out (`StreamLiveKitRooms`) to make the cost visible as one permanent leaked poll per screen visit. A handler whose stream can be silent must still watch `tx.closed()` itself — fixing the pump is necessary, not sufficient. [server-streaming.md](../server-streaming.md). (tddy-codegen)

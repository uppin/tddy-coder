# Server-streaming teardown

A contract every server-streaming RPC in the repo depends on, and which was silently broken until
2026-08-14.

## The generated response pump

For a streaming method, `generator.rs` emits a task that drains the handler's stream into a bounded
channel the transport reads from:

```rust
tokio::spawn(async move {
    let mut stream = stream;
    while let Some(item) = stream.next().await {
        if tx.send(item.map(|r| r.encode_to_vec())).await.is_err() {
            break;
        }
    }
});
```

**The `break` is load-bearing.** It was previously `let _ = tx.send(...).await;` — the send error was
discarded and the loop kept pulling from a stream whose consumer had gone.

## Why discarding the error was a leak, not just waste

The chain, for any streaming handler that owns a task:

1. The handler holds a sender and returns a stream wrapping the matching receiver.
2. This pump drains that stream into `tx`.
3. On peer disconnect the transport aborts `forward_response_body`, dropping the pump's receiver.
4. With the error discarded, the pump kept awaiting `stream.next()` — so it kept the *handler's*
   receiver alive, so the handler's sender was never closed, so the handler's own "has my subscriber
   gone?" check could never fire.

The handler's task therefore ran until the process exited. Every subscription ever opened added
another one.

`StreamHostStats` had the same shape and got away with it, because it emits every 5 s regardless: its
send eventually failed for other reasons, and its work is a cheap local `sysinfo` read.
`StreamLiveKitRooms` is what made the cost visible — it is **idle by design** (a poll tick with no
delta emits nothing), so its send was never even attempted, and its work is remote HTTP fan-out at
3 s. Each visit to one screen leaked a permanent poll of an external server.

## What a handler still owes

Fixing the pump is necessary but not sufficient. A handler whose stream can be silent for long
periods must **also** watch for its subscriber going away directly, rather than inferring it from a
failed send that may never be attempted:

```rust
tokio::select! {
    _ = tx.closed() => return,
    _ = poll.tick()  => {}
}
```

See `pump_rooms` in `tddy-daemon`'s `livekit_rooms_stream.rs` for the worked example, and
[connection-service.md § LiveKit rooms](../../tddy-daemon/docs/connection-service.md).

## Testing note

There is no test harness that exercises the generated adapter directly — `tddy-service` has no
`tests/` directory, and `tddy-rpc`'s `server_engine_peer_disconnect.rs` operates below the generated
layer. The behaviour is pinned end-to-end instead, by
`tddy-daemon/tests/stream_livekit_rooms_rpc.rs::stops_reading_the_server_once_the_subscriber_is_gone`,
which drops a stream and asserts the roster stops being read. Anyone changing this pump should expect
that test — not a codegen test — to be the one that catches a regression.

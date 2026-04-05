# Fix: tddy-web terminal keystrokes arrive out of order

## Symptom

Keystrokes from the embedded terminal (tddy-web → LiveKit → daemon PTY) can appear **out of sequence** when typing quickly or editing a line, breaking shells and TUIs.

## Investigation notes

- **Web client (`GhosttyTerminalLiveKit`)**: `onData` enqueues bytes in order; `inputGen` drains the queue with `splice(0)` and concatenates chunks in FIFO order before each yield. Batching is not expected to reorder.
- **Connect transport (`tddy-livekit-web`)**: `handleBidiStreaming` publishes each client chunk in `for await` order; no intentional reorder.
- **Server (`tddy-livekit`)**: Each `RoomEvent::DataReceived` previously ran `handle_incoming` inside **`tokio::spawn`**. Independent tasks could reach `input_tx.send` for the same bidirectional RPC session in a different order than packets arrived, so the terminal service saw **scrambled input bytes**.

## Root cause

**Parallel dispatch of `DataReceived` handlers** allowed bidirectional stream chunks to be forwarded to the RPC bridge **out of order**.

## Fix

Process each RPC payload **sequentially** in the room event loop: `await Self::handle_incoming(...)` for `DataReceived` (same as already done for buffered `pending_data` on `ParticipantConnected`).

## Tests

- **Rust** (`packages/tddy-livekit/tests/rpc_scenarios.rs`): rapid sequential `BidiStreamSender::send` of many numbered payloads; assert `CountingEchoService` responses have `seq` and `msg` in strict order (guards regression).

## Verification

```bash
./dev cargo test -p tddy-livekit --test rpc_scenarios
```

Full workspace: `./test` or `./verify` per AGENTS.md.

**Note:** Run the above when the build environment has sufficient disk space; the ordering scenario is the `bidi-input-order` room block (64 rapid bidi sends).

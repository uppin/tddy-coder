# Changeset: livekit-rpc-chunking — wire the chunking codec into the LiveKit RPC transport (Rust + web)

**Date:** 2026-07-23  
**Branch:** `feat/livekit-rpc-chunking-wiring`  
**Packages:** `tddy-livekit`, `tddy-livekit-web`  
**Builds on:** PR #322 (`feat(tddy-livekit): chunking codec for oversized RPC frames`) — the pure codec, which nothing called.

## Problem

Production `tddy-daemon` was wedged: a LiveKit data-channel message larger than the negotiated
SCTP max (64 000 bytes) is rejected on `publish_data`, and `SharedPublisher` retries the same
doomed publish 30× forever. Oversized `StreamSessionActivity` snapshots (a 369 KB tool-result
record, plus 71/117/187 KB others) flooded the log with
`data packet size … exceeds the negotiated maximum message size` and starved small RPCs like
`ListSessions`, so the web dashboard showed protobuf decode errors (`premature EOF`,
`cant skip wire type 4`) and appeared stuck.

PR #322 shipped `packages/tddy-livekit/src/chunking.rs` as a tested but **unwired** module — a
redeploy of it changed nothing at runtime. This changeset wires it in end-to-end.

## Design

- **Back-compat wire format.** A payload that fits in one packet is sent **raw** (byte-for-byte the
  encoded envelope), so a pre-chunking peer still decodes small messages. Only an oversized payload
  is split; each chunk frame begins with a `0x00` magic byte + 13-byte LE header
  `[magic][message_id][total_chunks][index]`. A valid `prost` `RpcRequest`/`RpcResponse` can never
  begin with `0x00` (protobuf field 0 is illegal; the envelopes' lowest fields are 1/2), so the
  receiver disambiguates raw vs chunk on the first byte (`is_chunk_frame`).
- **Per-sender reassembly.** `message_id` is unique only within a single sender, so each receive
  loop keeps one `ChunkReassembler` per sender identity (a browser talks to several daemons on one
  room). Reassembly is index-keyed, so frames need not arrive in order.
- **Symmetric TS mirror.** `packages/tddy-livekit-web/src/chunking.ts` matches the Rust codec
  byte-for-byte; both suites test the same fixtures (370 KB split/reassemble, interleaving, magic
  detection, raw passthrough, magic-collision forcing).

## Delta summary

### `tddy-livekit`

- `src/chunking.rs` — added `CHUNK_FRAME_MAGIC` (0x00) + magic-prefixed 13-byte header,
  `is_chunk_frame`, `frame_for_transport` (raw when it fits, chunk when oversized),
  `next_message_id` (process-wide `AtomicU32`), `Display`/`Error` for `ChunkError`.
- `src/client.rs` — `publish_request` frames the request via `frame_for_transport`, publishing one
  packet per frame; the response loop reassembles (single reassembler — filtered to one target).
- `src/client_factory.rs` — the shared per-room response loop reassembles per sender
  (`HashMap<Option<String>, ChunkReassembler>`), keyed by the `DataReceived` participant identity.
- `src/participant.rs` — both response drains (`spawn_response_drain`,
  `spawn_response_drain_reconnectable`) frame responses; the server event loop reassembles per
  sender via a new `reassemble` helper, applied on both the live and pre-connect-buffered paths.

### `tddy-livekit-web`

- `src/chunking.ts` (new) — TS port of the codec (`splitIntoFrames`, `ChunkReassembler`,
  `isChunkFrame`, `frameForTransport`, `nextMessageId`).
- `src/transport.ts` — `publishRequest` frames per `messageId`; `RoomRpcRegistry` reassembles per
  sender (listener now forwards participant identity); the standalone `LiveKitTransport` listener
  reassembles with a single per-target reassembler.

## Unit tests

- [x] `packages/tddy-livekit/src/chunking.rs` — 14 codec tests (25 lib tests total, all green)
- [x] `packages/tddy-livekit-web/src/chunking.test.ts` — 14 codec tests (26 pkg tests total, green)

## Follow-ups

- [ ] Cypress e2e crossing the TS↔Rust boundary with an oversized (~370 KB) payload against the
  real echo server (`transport.cy.tsx` + `livekitDockerTestkit`).
- [ ] Separately tracked (deferred): cap `result_json`/`input_json` at the activity-log layer so
  individual records stay small regardless of transport (`tddy-daemon/src/tool_call_log.rs`).

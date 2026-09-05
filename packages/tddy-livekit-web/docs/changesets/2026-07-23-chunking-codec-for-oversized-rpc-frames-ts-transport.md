# 2026-07-23 — Chunking codec for oversized RPC frames (TS transport)

**Type:** Feature

`src/chunking.ts` mirrors the Rust codec byte-for-byte (`frameForTransport`, `ChunkReassembler`, `isChunkFrame`, `nextMessageId`); `transport.ts` frames on `publishRequest` and reassembles per sender (`RoomRpcRegistry` + the standalone `LiveKitTransport`). Oversized (>64,000-byte SCTP max) frames split with a `0x00` magic-byte + 13-byte LE header; small payloads stay raw for back-compat, disambiguated on the first byte. Pairs with the Rust wiring so the web dashboard no longer sees `premature EOF` / protobuf decode errors when a daemon publishes large snapshots. 14 codec tests (`chunking.test.ts`). Feature [rpc-multi-transport.md § LiveKit oversized-frame chunking](../../../../docs/ft/coder/rpc-multi-transport.md#livekit-oversized-frame-chunking). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#323](https://github.com/uppin/tddy-coder/pull/323). (tddy-livekit-web)

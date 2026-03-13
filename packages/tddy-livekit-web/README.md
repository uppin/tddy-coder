# tddy-livekit-web

ConnectRPC Transport over LiveKit data channels for browser-based TypeScript clients.

## Overview

Implements the ConnectRPC `Transport` interface over LiveKit data channels using the `tddy-rpc` envelope protocol. Enables any ConnectRPC client to call unary and streaming RPCs served by a Rust `LiveKitParticipant`.

## Features

- **LiveKitTransport** — ConnectRPC Transport with `unary()` and `stream()` methods
- **AsyncQueue** — Backpressure-aware async channel for streaming responses
- **Streaming support** — Unary, client streaming, server streaming, bidirectional streaming

## Usage

```typescript
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { createLiveKitTransport } from "tddy-livekit-web";
import { EchoService } from "./gen/echo_pb.js";

const room = new Room();
await room.connect(url, token);

const transport = createLiveKitTransport({
  room,
  targetIdentity: "server",
});

const client = createClient(EchoService, transport);
const response = await client.echo({ message: "hello" });
```

## Dependencies

- `livekit-client` — Room connection and data channel I/O
- `@bufbuild/protobuf` — Protobuf serialization
- `@connectrpc/connect` — Transport interface

## Testing

Cypress component tests run against a real Rust echo server. Requires `LIVEKIT_TESTKIT_WS_URL` and pre-built `target/debug/examples/echo_server`:

```bash
eval $(./run-livekit-testkit-server | grep '^export ')
cd packages/tddy-livekit-web && bun run cypress:component
```

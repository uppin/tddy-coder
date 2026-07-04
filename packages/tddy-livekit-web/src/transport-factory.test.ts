/**
 * A per-room ConnectRPC-over-LiveKit transport factory — the browser mirror of the Rust
 * `LiveKitRpcClientFactory`. A browser holds one common-room connection but talks to several daemons
 * (and re-targets on daemon switch); building an independent `LiveKitTransport` per target each adds
 * its own `DataReceived` listener and its own request-id space (a module-global counter today). The
 * factory owns ONE shared listener and ONE per-connection request-id registry, and vends transports
 * per target that share them.
 */

import { describe, it, expect } from "bun:test";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import { RoomEvent } from "livekit-client";
import { RpcRequestSchema, RpcResponseSchema } from "./gen/rpc_envelope_pb.js";
import { LiveKitTransportFactory } from "./transport.js";

const FAKE_METHOD = {
  kind: "unary",
  name: "Ping",
  parent: { typeName: "test.TestService" },
  input: RpcRequestSchema,
  output: RpcResponseSchema,
} as any;

type DataReceivedListener = (
  payload: Uint8Array,
  participant?: { identity: string } | null,
  _kind?: unknown,
  topic?: string,
) => void;

function makeFakeRoom() {
  const listeners: Record<string, DataReceivedListener[]> = {};
  const published: Array<{ payload: Uint8Array; destinations: string[] }> = [];
  const room = {
    on(event: string, handler: DataReceivedListener) {
      (listeners[event] ??= []).push(handler);
      return room;
    },
    off(event: string, handler: DataReceivedListener) {
      const arr = listeners[event] ?? [];
      const i = arr.indexOf(handler);
      if (i >= 0) arr.splice(i, 1);
      return room;
    },
    localParticipant: {
      identity: "web-client",
      publishData(payload: Uint8Array, opts: { destinationIdentities?: string[] }) {
        published.push({ payload, destinations: opts?.destinationIdentities ?? [] });
      },
    },
    _emit(event: string, payload: Uint8Array, participant: { identity: string } | null, topic: string) {
      for (const handler of (listeners[event] ?? []).slice()) {
        handler(payload, participant, undefined, topic);
      }
    },
    _listenerCount(event: string) {
      return (listeners[event] ?? []).length;
    },
    _published() {
      return published;
    },
  };
  return room;
}

/** Build an RpcResponse whose inner `responseMessage` marks which call it belongs to. */
function makeMarkedResponse(requestId: number, marker: number): Uint8Array {
  const innerMessage = toBinary(RpcResponseSchema, create(RpcResponseSchema, { requestId: marker }));
  const response = create(RpcResponseSchema, {
    requestId,
    responseMessage: innerMessage,
    endOfStream: true,
  });
  return toBinary(RpcResponseSchema, response);
}

/** The request id the factory published for a given target. */
function publishedRequestIdForTarget(room: ReturnType<typeof makeFakeRoom>, target: string): number {
  const entry = room._published().find((p) => p.destinations.includes(target));
  if (!entry) throw new Error(`no published request for target ${target}`);
  return fromBinary(RpcRequestSchema, entry.payload).requestId;
}

describe("LiveKitTransportFactory", () => {
  it("registers a single DataReceived listener no matter how many transports it vends", () => {
    // Given — a factory for a room
    const room = makeFakeRoom();
    const factory = LiveKitTransportFactory.forRoom(room as any);

    // When — several transports are vended for different targets
    factory.transport("daemon-a");
    factory.transport("daemon-b");
    factory.transport("daemon-c");

    // Then — exactly one shared inbound listener is registered on the room, not one per transport
    expect(room._listenerCount(RoomEvent.DataReceived)).toBe(1);
  });

  it("returns the same factory instance for the same room", () => {
    // Given / When / Then — the factory is a singleton per connection
    const room = makeFakeRoom();
    expect(LiveKitTransportFactory.forRoom(room as any)).toBe(
      LiveKitTransportFactory.forRoom(room as any),
    );
  });

  it("gives each connection its own request-id sequence starting at 1", async () => {
    // Given — one connection that has already issued two calls
    const busyRoom = makeFakeRoom();
    const busyFactory = LiveKitTransportFactory.forRoom(busyRoom as any);
    void busyFactory.transport("daemon-a").unary(FAKE_METHOD, undefined, undefined, undefined, {});
    void busyFactory.transport("daemon-a").unary(FAKE_METHOD, undefined, undefined, undefined, {});
    await Promise.resolve();

    // When — a fresh connection issues its first call
    const freshRoom = makeFakeRoom();
    const freshFactory = LiveKitTransportFactory.forRoom(freshRoom as any);
    void freshFactory.transport("daemon-x").unary(FAKE_METHOD, undefined, undefined, undefined, {});
    await Promise.resolve();

    // Then — the fresh connection starts at 1, independent of the busy connection's counter
    expect(publishedRequestIdForTarget(freshRoom, "daemon-x")).toBe(1);
  });

  it("routes concurrent responses to the transport that made each call, never crossing targets", async () => {
    // Given — one factory vending transports to two daemons
    const room = makeFakeRoom();
    const factory = LiveKitTransportFactory.forRoom(room as any);
    const toA = factory.transport("daemon-a");
    const toB = factory.transport("daemon-b");

    // When — both call concurrently and the responses arrive out of order
    const callA = toA.unary(FAKE_METHOD, undefined, undefined, undefined, {});
    const callB = toB.unary(FAKE_METHOD, undefined, undefined, undefined, {});
    await Promise.resolve();
    const idA = publishedRequestIdForTarget(room, "daemon-a");
    const idB = publishedRequestIdForTarget(room, "daemon-b");
    room._emit(RoomEvent.DataReceived, makeMarkedResponse(idB, 222), { identity: "daemon-b" }, "tddy-rpc");
    room._emit(RoomEvent.DataReceived, makeMarkedResponse(idA, 111), { identity: "daemon-a" }, "tddy-rpc");

    // Then — each call resolves to its own daemon's response
    const resultA = await callA;
    const resultB = await callB;
    expect((resultA.message as { requestId: number }).requestId).toBe(111);
    expect((resultB.message as { requestId: number }).requestId).toBe(222);
  });
});

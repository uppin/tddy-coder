/**
 * Regression tests: a response minted for a previous browser connection must never be delivered to
 * a call of the current one.
 *
 * Every RPC to every daemon is multiplexed over one LiveKit data-channel topic and correlated by
 * `request_id` alone (`RoomRpcRegistry.route`). Three properties compose into a silent data leak:
 *
 *   1. `RpcResponse` carries no call identity — unlike `RpcRequest`, which carries `call_metadata`
 *      (service + method). A response cannot say which call it answers.
 *   2. The registry's `pendingStreams` maps a request id to an `AsyncQueue<Uint8Array>` — raw
 *      bytes. The queue does not know its message type; the caller decodes whatever arrives as
 *      whatever *that* call expects.
 *   3. The id counter is per-registry and starts at 1, so it restarts on every page load, while
 *      the participant identity is persisted in `sessionStorage` (`presenceIdentity.ts`) and
 *      survives the reload. The daemon keeps publishing its still-open streams to the same
 *      identity, tagged with the *previous* page's request ids.
 *
 * So after a reload, a stale `StreamTerminalOutput` tagged `request_id=3` lands in whatever the new
 * page registered as id 3. When that is `WatchTerminalControl`, the PTY bytes are decoded as a
 * `TerminalControlEvent`: `SessionTerminalOutput.data` (`bytes`, field 1) and
 * `TerminalControlEvent.holder_screen_id` (`string`, field 1) share a field number *and* a wire
 * type, so it decodes cleanly and the terminal's output is rendered as the lease holder's screen id.
 *
 * The shared field number is not the cause — it is only why the corruption is legible instead of a
 * decode error. The cause is unvalidated correlation, and it corrupts *any* pair of streams: a
 * stale output stream for one session landing in a new output stream for another would silently
 * show the wrong session's terminal, with no garbage to notice.
 *
 * The fix is a per-connection `client_epoch` stamped on every request and echoed on every response,
 * with a mismatch dropped rather than delivered.
 */

import { describe, it, expect } from "bun:test";
import { create, toBinary } from "@bufbuild/protobuf";
import { RoomEvent } from "livekit-client";
import { RpcResponseSchema, CallMetadataSchema } from "./gen/rpc_envelope_pb.js";
import { RoomRpcRegistry } from "./transport.js";
import { AsyncQueue } from "./async-queue.js";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const RPC_TOPIC = "tddy-rpc";

/** The epoch of the page that is open now. */
const THIS_CONNECTION = 0x5f3a91c2;
/** The epoch of the page that was open before the reload, whose streams the daemon still serves. */
const THE_RELOADED_AWAY_CONNECTION = 0x11ac07e4;

/**
 * A PTY frame as captured in the report: erase-line and cursor-column escapes, the tail of a Claude
 * Code TUI menu, CRLFs. Encoded exactly as `SessionTerminalOutput.data` — `bytes`, field 1 — which
 * is byte-for-byte what a `TerminalControlEvent` decoder reads as `holder_screen_id`.
 */
const A_TERMINAL_OUTPUT_FRAME =
  "\u001b[2K\u001b[1G  6. Chat about this\r\n\u001b[2m  Enter to select · ↑/↓ to navigate · Esc to cancel\u001b[0m\r\n";

/** Encode `text` as protobuf field 1, length-delimited — the wire shape both messages share. */
function asFieldOneBytes(text: string): Uint8Array {
  const payload = new TextEncoder().encode(text);
  const header = [0x0a]; // field 1, wire type 2
  let len = payload.length;
  do {
    const byte = len & 0x7f;
    len >>>= 7;
    header.push(len > 0 ? byte | 0x80 : byte);
  } while (len > 0);
  return new Uint8Array([...header, ...payload]);
}

// ---------------------------------------------------------------------------
// Fake room
// ---------------------------------------------------------------------------

type DataReceivedListener = (
  payload: Uint8Array,
  participant?: { identity: string } | null,
  kind?: unknown,
  topic?: string,
) => void;

function aRoom() {
  const listeners: DataReceivedListener[] = [];
  return {
    on(event: string, handler: DataReceivedListener) {
      if (event === RoomEvent.DataReceived) listeners.push(handler);
      return this;
    },
    off() {
      return this;
    },
    localParticipant: {
      identity: "web-alice-1755100800000-k3f9qz",
      publishData() {},
    },
    /** Deliver one already-framed `RpcResponse` payload on the RPC topic. */
    deliver(payload: Uint8Array) {
      for (const handler of listeners) {
        handler(payload, { identity: "daemon-udoo" }, undefined, RPC_TOPIC);
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

function aBrowserConnection(clientEpoch: number) {
  const room = aRoom();
  const registry = new RoomRpcRegistry(room as never, false, undefined, clientEpoch);

  return {
    /** Register a server-streaming call under `requestId`, as `handleServerStreaming` does. */
    openStream(requestId: number) {
      const queue = new AsyncQueue<Uint8Array>();
      registry.pendingStreams.set(requestId, {
        call: {
          service: "connection.ConnectionService",
          method: "WatchTerminalControl",
        },
        queue,
      });
      return queue;
    },
    /** Deliver a response frame from the daemon, tagged with `epoch`'s connection. */
    receiveStreamFrame(requestId: number, epoch: number, message: Uint8Array, method: string) {
      const response = create(RpcResponseSchema, {
        requestId,
        responseMessage: message,
        endOfStream: false,
        clientEpoch: epoch,
        callMetadata: create(CallMetadataSchema, {
          service: "connection.ConnectionService",
          method,
        }),
      });
      room.deliver(toBinary(RpcResponseSchema, response));
    },
  };
}

/**
 * Everything the stream actually received. Closing first bounds the drain: `dequeue` still hands
 * back buffered items before reporting the close, and never blocks waiting for one that will not
 * come.
 */
async function delivered(queue: AsyncQueue<Uint8Array>): Promise<Uint8Array[]> {
  queue.close();
  const items: Uint8Array[] = [];
  for await (const item of queue) items.push(item);
  return items;
}

// ---------------------------------------------------------------------------

describe("stale-connection response correlation", () => {
  it("drops a terminal-output frame left over from the connection before the reload", async () => {
    // Given — this page opened WatchTerminalControl and it was allocated request id 3, the same id
    // the previous page's StreamTerminalOutput still holds on the daemon.
    const connection = aBrowserConnection(THIS_CONNECTION);
    const controlStream = connection.openStream(3);

    // When — the daemon publishes a frame of that still-open output stream to our identity.
    connection.receiveStreamFrame(
      3,
      THE_RELOADED_AWAY_CONNECTION,
      asFieldOneBytes(A_TERMINAL_OUTPUT_FRAME),
      "StreamTerminalOutput",
    );

    // Then — nothing reaches the control stream. Delivered, it would decode as a
    // TerminalControlEvent whose holder_screen_id is the terminal's own output.
    expect(await delivered(controlStream)).toEqual([]);
  });

  it("delivers a frame minted by this connection", async () => {
    // Given — a stream open on this connection
    const connection = aBrowserConnection(THIS_CONNECTION);
    const controlStream = connection.openStream(3);
    const event = asFieldOneBytes("screen-1755100800000-k3f9qz");

    // When — the daemon answers the call this connection actually made
    connection.receiveStreamFrame(3, THIS_CONNECTION, event, "WatchTerminalControl");

    // Then
    expect(await delivered(controlStream)).toEqual([event]);
  });

  it("drops a frame answering a different method on the same request id", async () => {
    // Given — this connection's id 3 is WatchTerminalControl
    const connection = aBrowserConnection(THIS_CONNECTION);
    const controlStream = connection.openStream(3);

    // When — a frame arrives with this connection's epoch but naming another method. Same-epoch
    // crossing is not the reported bug, but the response names its call now, so it costs nothing
    // to refuse a mismatch instead of decoding one message as another.
    connection.receiveStreamFrame(
      3,
      THIS_CONNECTION,
      asFieldOneBytes(A_TERMINAL_OUTPUT_FRAME),
      "StreamTerminalOutput",
    );

    // Then
    expect(await delivered(controlStream)).toEqual([]);
  });
});

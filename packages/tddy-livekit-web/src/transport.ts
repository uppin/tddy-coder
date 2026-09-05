/**
 * LiveKit ConnectRPC Transport - bridges ConnectRPC clients to Rust RPC services
 * over LiveKit data channels using the tddy-rpc envelope protocol.
 *
 * The envelope itself — request ids, the per-connection client epoch, correlating a response with
 * the call that made it, settling pending calls, turning response frames into async iterables — is
 * `tddy-rpc-web`'s {@link EnvelopeTransport}, shared with every other flavour. What lives here is
 * what makes this flavour LiveKit: the data-channel topic, chunking a request that outgrows one
 * packet, reassembling inbound chunks per sender, the room's single `DataReceived` listener, and
 * failing the calls in flight to a peer that leaves the room.
 */

import {
  ConnectError,
  Code,
  type Transport,
  type UnaryResponse,
  type StreamResponse,
} from "@connectrpc/connect";
import type {
  DescMessage,
  DescMethodUnary,
  DescMethodStreaming,
  MessageInitShape,
} from "@bufbuild/protobuf";
import { RoomEvent, type Room } from "livekit-client";
import createDebug from "debug";
import {
  EnvelopeTransport,
  PendingCalls,
  type PendingCall,
  type PendingStreamCall,
  type PendingUnaryCall,
} from "tddy-rpc-web";
import {
  ChunkReassembler,
  frameForTransport,
  isChunkFrame,
  nextMessageId,
} from "./chunking.js";

const RPC_TOPIC = "tddy-rpc";

/** Enable via DevTools `localStorage.debug = 'tddy:rpc:*'` (or `dev.daemon.yaml` `debug`, served at
 *  `/api/config` — see `tddy-web`'s `debugMask.ts`). */
const registryLog = createDebug("tddy:rpc:room-rpc-registry");
const transportLog = createDebug("tddy:rpc:livekit-transport");

export type { PendingCall };

/** Notified when an inbound frame cannot be turned into a response, with a short `context` naming
 *  what failed and which sender it came from — see {@link LiveKitTransportOptions.onTransportError}. */
export type TransportErrorHandler = (error: unknown, context: string) => void;

/**
 * Surface a failure to turn inbound bytes into a response. Such a frame is a real fault, not noise:
 * a decode failure is how a reassembly bug reaches the app (`refresh error RangeError: premature
 * EOF`), and swallowing it leaves a call that never settles with nothing to point at. Reported to
 * the host's handler when it supplied one, otherwise logged — never dropped.
 */
function reportTransportError(
  onTransportError: TransportErrorHandler | undefined,
  context: string,
  error: unknown,
): void {
  if (onTransportError) {
    onTransportError(error, context);
    return;
  }
  console.error(`[tddy-rpc] ${context}:`, error);
}

/**
 * Inbound chunk reassembly grouped per sender: a `messageId` is only unique within one sender (every
 * sender's counter starts at 0), so frames from different senders must never land in the same
 * message. The `undefined` bucket is load-bearing — livekit-client reports no participant whenever
 * the sender is not (yet) in `room.remoteParticipants`, and those frames must not be able to
 * complete an identified peer's message.
 */
class InboundReassembly {
  private readonly bySender = new Map<string | undefined, ChunkReassembler>();

  /**
   * Accept one received frame from `sender`: a raw envelope passes through unchanged, a chunk frame
   * is buffered in `sender`'s own reassembler until its message completes. Returns `null` while
   * chunks are outstanding, and throws `ChunkError` on a malformed chunk frame.
   */
  accept(payload: Uint8Array, sender?: string): Uint8Array | null {
    if (!isChunkFrame(payload)) return payload;
    let reassembler = this.bySender.get(sender);
    if (!reassembler) {
      reassembler = new ChunkReassembler();
      this.bySender.set(sender, reassembler);
    }
    return reassembler.accept(payload);
  }

  clear(): void {
    this.bySender.clear();
  }
}

/**
 * Shared per-room RPC state: one `DataReceived` listener and the connection's own request-id space
 * and pending calls, which every transport vended for the room shares. A browser holds one
 * common-room connection but talks to several daemons; routing all of them through one registry
 * means a single listener (not one per client), a single id space (so a response id maps to exactly
 * one pending call regardless of which daemon replied — no sender filter needed), and no cross-talk
 * between clients. Mirrors the Rust `LiveKitRpcClientFactory`.
 */
export class RoomRpcRegistry {
  /** The connection's id space and the calls registered in it, shared with its transports. */
  readonly calls: PendingCalls;
  private listener:
    | ((
        payload: Uint8Array,
        participant?: { identity?: string } | null,
        kind?: unknown,
        topic?: string,
      ) => void)
    | null = null;
  private departureListener: ((participant?: { identity?: string } | null) => void) | null = null;
  // Reassemble chunked responses from *every* peer in the room (a browser may talk to several
  // daemons), so frames are grouped per sender — message ids are only unique within one sender.
  private readonly reassembly = new InboundReassembly();

  constructor(
    private readonly room: Room,
    private readonly debug = false,
    private readonly onTransportError?: TransportErrorHandler,
    /** This connection's identity, stamped on every request and required on every response. */
    clientEpoch?: number,
    /** Inbound byte meter. Set only by a transport that owns its registry; a registry shared across
     *  a room's transports cannot attribute an inbound frame to one of them. */
    private readonly meter?: { record(dir: "in" | "out", bytes: number): void },
  ) {
    this.calls = new PendingCalls(
      clientEpoch,
      debug ? (message: string) => transportLog(message) : undefined,
    );

    this.listener = (
      payload: Uint8Array,
      participant?: { identity?: string } | null,
      _kind?: unknown,
      topic?: string,
    ) => {
      if (topic !== RPC_TOPIC) return;
      this.meter?.record("in", (payload as Uint8Array).length);
      this.route(payload as Uint8Array, participant?.identity);
    };
    this.room.on(RoomEvent.DataReceived, this.listener as any);

    // A response is the only other thing that settles a call, so a peer that leaves the room takes
    // every call in flight to it with it — otherwise those callers wait for an answer nobody is left
    // to send.
    this.departureListener = (participant?: { identity?: string } | null) => {
      this.failCallsTo(participant?.identity);
    };
    this.room.on(RoomEvent.ParticipantDisconnected, this.departureListener as any);
  }

  /** This connection's identity. Every response must carry it to be delivered. */
  get clientEpoch(): number {
    return this.calls.clientEpoch;
  }

  get pendingUnary(): Map<number, PendingUnaryCall> {
    return this.calls.pendingUnary;
  }

  get pendingStreams(): Map<number, PendingStreamCall> {
    return this.calls.pendingStreams;
  }

  allocateRequestId(): number {
    return this.calls.allocateRequestId();
  }

  /**
   * Settle the call registered under `requestId` as a failure — a deadline, a departed peer, a
   * request that never went out — and release its registration. A no-op once the call has settled.
   */
  failCall(requestId: number, error: ConnectError): void {
    this.calls.failCall(requestId, error);
  }

  /** Settle the call registered under `requestId` as cancelled by its own caller. */
  cancelCall(requestId: number): void {
    this.calls.cancelCall(requestId);
  }

  /** Fail every call in flight to `identity`, and only those — the request-id space spans all of the
   *  room's peers. Called with no identity, nothing is failed: an unnamed departure says nothing
   *  about which calls can no longer be answered. */
  private failCallsTo(identity: string | undefined): void {
    if (!identity) return;
    // Collected before failing any: failing a call mutates the map it was found in.
    for (const requestId of this.calls.requestIdsMatching((call) => call.target === identity)) {
      if (this.debug) registryLog(`failing request_id=${requestId}: ${identity} left the room`);
      this.failCall(requestId, new ConnectError(`${identity} left the room`, Code.Unavailable));
    }
  }

  private route(payload: Uint8Array, sender?: string): void {
    const full = this.reassemble(payload, sender);
    if (full === null) return;
    try {
      this.calls.deliverFrame(full);
    } catch (e) {
      if (this.debug) registryLog(`decode error:`, e);
      reportTransportError(this.onTransportError, `decode error from sender=${sender}`, e);
    }
  }

  /** Reassemble one frame from `sender` through that sender's own reassembler. Returns `null` while
   *  chunks are outstanding, or when the frame is malformed (reported and dropped, so one bad frame
   *  never tears down the room's listener). */
  private reassemble(payload: Uint8Array, sender?: string): Uint8Array | null {
    try {
      return this.reassembly.accept(payload, sender);
    } catch (e) {
      if (this.debug) registryLog(`malformed chunk frame:`, e);
      reportTransportError(this.onTransportError, `malformed chunk frame from sender=${sender}`, e);
      return null;
    }
  }

  dispose(): void {
    if (this.listener) {
      this.room.off(RoomEvent.DataReceived, this.listener as any);
      this.listener = null;
    }
    if (this.departureListener) {
      this.room.off(RoomEvent.ParticipantDisconnected, this.departureListener as any);
      this.departureListener = null;
    }
    // Dropping the pending calls would orphan every awaiter: correlation is gone, so no later frame
    // can ever settle them. They are cancelled instead, which is what tearing the room down is.
    for (const requestId of this.calls.requestIdsInFlight()) {
      this.failCall(requestId, new ConnectError("room RPC state disposed", Code.Canceled));
    }
    this.reassembly.clear();
  }
}

export interface LiveKitTransportOptions {
  room: Room;
  targetIdentity: string;
  debug?: boolean;
  /** Optional traffic meter for recording inbound/outbound payload bytes. */
  meter?: { record(dir: "in" | "out", bytes: number): void };
  /**
   * Shared per-room registry (from {@link LiveKitTransportFactory}). When provided, this transport
   * routes correlation through it — one listener and one request-id space shared with every other
   * transport on the room — instead of installing its own listener and id space. Omit for a
   * standalone transport.
   */
  registry?: RoomRpcRegistry;
  /**
   * Called with every inbound frame this transport could not turn into a response — a malformed
   * chunk frame, or bytes that do not decode as an `RpcResponse`. The frame is still dropped and the
   * listener survives; the handler exists so the host can surface the fault instead of only seeing a
   * call that never settles. Without a handler the failure is logged to the console.
   */
  onTransportError?: TransportErrorHandler;
}

export class LiveKitTransport implements Transport {
  private room: Room;
  private targetIdentity: string;
  private debug: boolean;
  private meter: { record(dir: "in" | "out", bytes: number): void } | undefined;
  /**
   * Correlation state. Always present: a transport given no registry builds its own rather than
   * running a second, parallel routing path. One implementation of "does this response answer this
   * call" means a new dispatch path cannot be added that forgets to ask.
   */
  private registry: RoomRpcRegistry;
  /** True when this transport built {@link registry} itself and must dispose it. */
  private ownsRegistry: boolean;
  /** The envelope engine, sending through this transport's data-channel publish. */
  private envelope: EnvelopeTransport;

  constructor(options: LiveKitTransportOptions) {
    this.room = options.room;
    this.targetIdentity = options.targetIdentity;
    this.debug = options.debug ?? false;
    this.meter = options.meter;
    this.ownsRegistry = options.registry === undefined;
    this.registry =
      options.registry ??
      new RoomRpcRegistry(
        this.room,
        this.debug,
        options.onTransportError,
        undefined,
        this.meter,
      );

    this.envelope = new EnvelopeTransport({
      calls: this.registry.calls,
      // Request ids are allocated from one space shared by every daemon the room talks to, so a
      // peer's departure can only be turned into the right failures by target.
      target: this.targetIdentity,
      label: this.targetIdentity,
      senderIdentity: () => this.room.localParticipant.identity,
      log: this.debug ? (message: string) => transportLog(message) : undefined,
      sendFrame: (frame, requestId) => this.publishRequestOrFailCall(frame, requestId),
    });

    if (this.debug) {
      transportLog(
        `created, listening for DataReceived topic=${RPC_TOPIC} target=${this.targetIdentity} client_epoch=${this.registry.clientEpoch}`
      );
    }
  }

  /** This connection's identity. Every response must carry it to be delivered. */
  get clientEpoch(): number {
    return this.registry.clientEpoch;
  }

  unary<I extends DescMessage, O extends DescMessage>(
    method: DescMethodUnary<I, O>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: MessageInitShape<I>
  ): Promise<UnaryResponse<I, O>> {
    return this.envelope.unary(method, signal, timeoutMs, header, input);
  }

  stream<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>
  ): Promise<StreamResponse<I, O>> {
    return this.envelope.stream(method, signal, timeoutMs, header, input);
  }

  /**
   * Send `payload` to this transport's target. Resolves once every frame has been handed to LiveKit,
   * and rejects when one could not be — publishing on a room that has gone away fails.
   */
  private async publishRequest(payload: Uint8Array, requestId: number): Promise<void> {
    this.meter?.record("out", payload.length);
    if (this.debug) {
      transportLog(
        `publish request_id=${requestId} bytes=${payload.length} target=${this.targetIdentity}`
      );
    }
    // Fits-in-one-packet requests go out raw (unchanged wire bytes); an oversized request is split
    // into chunk frames that each fit LiveKit's negotiated max message size. The receiver's
    // reassembler is index-keyed, so frames need not arrive in order — they are published together
    // and awaited as one.
    const messageId = nextMessageId();
    await Promise.all(
      frameForTransport(messageId, payload).map((frame) =>
        this.room.localParticipant.publishData(frame, {
          reliable: true,
          topic: RPC_TOPIC,
          destinationIdentities: [this.targetIdentity],
        })
      )
    );
  }

  /**
   * Send `payload`, failing the call registered under its request id if it never went out.
   *
   * A request that was not published has no answer coming, so its caller has to be told — and the
   * rejection has to be observed either way, or a failed publish surfaces as an unhandled rejection
   * with a call left hanging beside it.
   */
  private publishRequestOrFailCall(payload: Uint8Array, requestId: number): void {
    void this.publishRequest(payload, requestId).catch((error) => {
      if (this.debug) {
        transportLog(`error request_id=${requestId} publish failed: ${String(error)}`);
      }
      this.registry.failCall(
        requestId,
        new ConnectError(
          `could not send request to ${this.targetIdentity}: ${ConnectError.from(error).rawMessage}`,
          Code.Unavailable
        )
      );
    });
  }

  destroy(): void {
    // A shared registry outlives this transport — it belongs to the room and its sibling
    // transports. Only a registry this transport built itself is torn down here.
    if (this.ownsRegistry) {
      this.registry.dispose();
    }
  }
}

export function createLiveKitTransport(options: LiveKitTransportOptions): Transport {
  return new LiveKitTransport(options);
}

/**
 * Singleton-per-room factory for ConnectRPC-over-LiveKit transports. All transports vended for a
 * given room share one {@link RoomRpcRegistry} — one `DataReceived` listener and one per-connection
 * request-id space — so a browser talking to several daemons over its single common-room connection
 * never leaks a listener per client nor crosses responses between targets. Mirrors the Rust
 * `LiveKitRpcClientFactory`.
 */
export class LiveKitTransportFactory {
  private static readonly byRoom = new WeakMap<Room, LiveKitTransportFactory>();

  private readonly registry: RoomRpcRegistry;

  private constructor(
    private readonly room: Room,
    private readonly debug: boolean,
    onTransportError?: TransportErrorHandler,
  ) {
    this.registry = new RoomRpcRegistry(room, debug, onTransportError);
  }

  /**
   * The factory for `room`, created once and reused for every later call with the same room. Because
   * the shared registry owns the room's single inbound listener, `debug` and `onTransportError` are
   * per-room settings fixed by whoever asks for the factory first; later callers get the existing
   * factory and their arguments are ignored.
   */
  static forRoom(
    room: Room,
    debug = false,
    onTransportError?: TransportErrorHandler,
  ): LiveKitTransportFactory {
    const existing = LiveKitTransportFactory.byRoom.get(room);
    if (existing) return existing;
    const created = new LiveKitTransportFactory(room, debug, onTransportError);
    LiveKitTransportFactory.byRoom.set(room, created);
    return created;
  }

  /** This connection's identity, shared by every transport vended for the room. */
  get clientEpoch(): number {
    return this.registry.clientEpoch;
  }

  /** Vend a transport that sends to `targetIdentity` over the room's shared registry. */
  transport(
    targetIdentity: string,
    options?: { meter?: { record(dir: "in" | "out", bytes: number): void } },
  ): Transport {
    return new LiveKitTransport({
      room: this.room,
      targetIdentity,
      debug: this.debug,
      meter: options?.meter,
      registry: this.registry,
    });
  }
}

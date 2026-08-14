/**
 * LiveKit ConnectRPC Transport - bridges ConnectRPC clients to Rust RPC services
 * over LiveKit data channels using the tddy-rpc envelope protocol.
 */

import {
  create,
  toBinary,
  fromBinary,
  type DescMessage,
  type DescMethodUnary,
  type DescMethodStreaming,
  type MessageInitShape,
} from "@bufbuild/protobuf";
import {
  ConnectError,
  Code,
  type Transport,
  type UnaryResponse,
  type StreamResponse,
} from "@connectrpc/connect";
import { codeFromString } from "@connectrpc/connect/protocol-connect";
import { RoomEvent, type Room } from "livekit-client";
import createDebug from "debug";
import {
  RpcRequestSchema,
  RpcResponseSchema,
  CallMetadataSchema,
  type RpcRequest,
  type RpcResponse,
  type RpcError,
} from "./gen/rpc_envelope_pb.js";
import { AsyncQueue } from "./async-queue.js";
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


/**
 * Distinguishes one client connection from the next.
 *
 * A request id restarts at 1 whenever a page builds a fresh connection, while the LiveKit
 * participant identity is persisted in `sessionStorage` and survives a reload — so the daemon keeps
 * publishing frames of streams the *previous* page opened, tagged with ids the new page is about to
 * hand out again. Without a per-connection discriminator those frames resolve whichever call now
 * holds the id, and their bytes are decoded as that call's message type. Never 0: a zero epoch on
 * the wire means the field was absent.
 */
function mintClientEpoch(): number {
  const epoch = Math.floor(Math.random() * 0xffffffff) >>> 0;
  return epoch === 0 ? 1 : epoch;
}

/** The call a pending entry was opened for, so an arriving response can be attributed to it. */
export interface PendingCall {
  service: string;
  method: string;
}

/**
 * Released when a pending entry is retired, whichever way the call ended, so the abort listener and
 * deadline timer a call was opened with never outlive it. Optional: a registration made with neither
 * has nothing to release.
 */
type ReleaseCallWatchers = () => void;

interface PendingUnaryCall {
  call: PendingCall;
  /**
   * The peer this call was sent to. Request ids are allocated from one space shared by every daemon
   * the room talks to, so a peer's departure can only be turned into the right failures by target —
   * failing by id alone would take another daemon's calls down with it.
   */
  target: string;
  resolve: (value: RpcResponse) => void;
  reject: (err: Error) => void;
  release?: ReleaseCallWatchers;
}

interface PendingStreamCall {
  call: PendingCall;
  /** The peer this call was sent to — see {@link PendingUnaryCall.target}. */
  target: string;
  queue: AsyncQueue<Uint8Array>;
  release?: ReleaseCallWatchers;
}


/**
 * `JSON.stringify` that survives `bigint` values. Decoded protobuf messages carry `uint64`/`int64`
 * fields as JS `BigInt` (e.g. `GetHostDiskStatsResponse.available_bytes`, `SessionEntry.bytes_in`),
 * which the built-in serializer throws on. Debug logging must never reject an otherwise-successful
 * RPC, so we stringify bigints as their decimal string form.
 */
function safeStringify(value: unknown): string {
  return JSON.stringify(value, (_key, v) => (typeof v === "bigint" ? v.toString() : v));
}

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
 * Shared per-room RPC state: one `DataReceived` listener, one request-id counter (scoped to this
 * connection, starting at 1), and the pending-call maps every transport vended for the room uses.
 * A browser holds one common-room connection but talks to several daemons; routing all of them
 * through one registry means a single listener (not one per client), a single id space (so a
 * response id maps to exactly one pending call regardless of which daemon replied — no sender
 * filter needed), and no cross-talk between clients. Mirrors the Rust `LiveKitRpcClientFactory`.
 */
export class RoomRpcRegistry {
  readonly pendingUnary = new Map<number, PendingUnaryCall>();
  readonly pendingStreams = new Map<number, PendingStreamCall>();
  private nextId = 1;
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
    readonly clientEpoch: number = mintClientEpoch(),
    /** Inbound byte meter. Set only by a transport that owns its registry; a registry shared across
     *  a room's transports cannot attribute an inbound frame to one of them. */
    private readonly meter?: { record(dir: "in" | "out", bytes: number): void },
  ) {
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

  allocateRequestId(): number {
    return this.nextId++;
  }

  /** Take the pending stream registered under `requestId` out of the map, releasing its watchers. */
  private retireStream(requestId: number): PendingStreamCall | undefined {
    const pending = this.pendingStreams.get(requestId);
    if (!pending) return undefined;
    this.pendingStreams.delete(requestId);
    pending.release?.();
    return pending;
  }

  /** Take the pending unary call registered under `requestId` out of the map, releasing its watchers. */
  private retireUnary(requestId: number): PendingUnaryCall | undefined {
    const pending = this.pendingUnary.get(requestId);
    if (!pending) return undefined;
    this.pendingUnary.delete(requestId);
    pending.release?.();
    return pending;
  }

  /**
   * Settle the call registered under `requestId` as a failure — a deadline, a departed peer, a
   * request that never went out — and release its registration. A no-op once the call has settled,
   * so a deadline that fires after its response arrived costs nothing.
   */
  failCall(requestId: number, error: ConnectError): void {
    const stream = this.retireStream(requestId);
    if (stream) {
      stream.queue.fail(error);
      return;
    }
    this.retireUnary(requestId)?.reject(error);
  }

  /**
   * Settle the call registered under `requestId` as cancelled by its own caller, and release its
   * registration.
   *
   * A cancelled stream *ends*: the caller asked for it to stop, so it has nothing left to be told,
   * and every consumer already treats stream-end as normal. A call whose response is a single
   * message has no such ending — there is no message to hand back — so it is rejected as
   * {@link Code.Canceled}.
   */
  cancelCall(requestId: number): void {
    const stream = this.retireStream(requestId);
    if (stream) {
      stream.queue.close();
      return;
    }
    this.retireUnary(requestId)?.reject(new ConnectError("call cancelled", Code.Canceled));
  }

  /** Fail every call in flight to `identity`, and only those — the request-id space spans all of the
   *  room's peers. Called with no identity, nothing is failed: an unnamed departure says nothing
   *  about which calls can no longer be answered. */
  private failCallsTo(identity: string | undefined): void {
    if (!identity) return;
    // Collected before failing any: failing a call mutates the map it was found in.
    const affected: number[] = [];
    for (const [requestId, pending] of this.pendingStreams) {
      if (pending.target === identity) affected.push(requestId);
    }
    for (const [requestId, pending] of this.pendingUnary) {
      if (pending.target === identity) affected.push(requestId);
    }
    for (const requestId of affected) {
      if (this.debug) registryLog(`failing request_id=${requestId}: ${identity} left the room`);
      this.failCall(requestId, new ConnectError(`${identity} left the room`, Code.Unavailable));
    }
  }

  /**
   * Whether `response` answers the call currently registered under its request id.
   *
   * A matching id is not enough. The daemon keeps serving streams opened by a connection that has
   * gone away and addresses them to the same participant identity, while ids restart from 1 — so an
   * id match alone can mean "a dead page's stream", and delivering it hands the caller another
   * call's bytes to decode as its own message type, with no error. That is how a terminal's output
   * came to be rendered as a control lease's holder screen id.
   */
  private answersItsCall(response: RpcResponse): boolean {
    if (response.clientEpoch !== this.clientEpoch) {
      if (this.debug) {
        transportLog(
          `dropping response request_id=${response.requestId} from client_epoch=${response.clientEpoch} (this connection is ${this.clientEpoch})`,
        );
      }
      return false;
    }
    const pending =
      this.pendingStreams.get(response.requestId) ?? this.pendingUnary.get(response.requestId);
    const answered = response.callMetadata;
    if (!pending || !answered) return true;
    if (pending.call.service === answered.service && pending.call.method === answered.method) {
      return true;
    }
    if (this.debug) {
      transportLog(
        `dropping response request_id=${response.requestId} answering ${answered.service}/${answered.method}, but that id holds ${pending.call.service}/${pending.call.method}`,
      );
    }
    return false;
  }

  private route(payload: Uint8Array, sender?: string): void {
    const full = this.reassemble(payload, sender);
    if (full === null) return;
    try {
      const response = fromBinary(RpcResponseSchema, full) as RpcResponse;
      if (!this.answersItsCall(response)) return;
      const requestId = response.requestId;
      const pendingStream = this.pendingStreams.get(requestId);
      if (pendingStream) {
        const streamQueue = pendingStream.queue;
        if (response.error) {
          this.retireStream(requestId);
          streamQueue.fail(rpcErrorToConnectError(response.error));
        } else {
          if (response.responseMessage && response.responseMessage.length > 0) {
            streamQueue.enqueue(response.responseMessage);
          }
          if (response.endOfStream) {
            this.retireStream(requestId);
            streamQueue.close();
          }
        }
        return;
      }
      const pending = this.retireUnary(requestId);
      if (pending) {
        pending.resolve(response);
      }
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
    // Dropping the maps would orphan every awaiter: correlation is gone, so no later frame can ever
    // settle these calls. They are cancelled instead, which is what tearing the room down is.
    const inFlight = [...this.pendingStreams.keys(), ...this.pendingUnary.keys()];
    for (const requestId of inFlight) {
      this.failCall(requestId, new ConnectError("room RPC state disposed", Code.Canceled));
    }
    this.reassembly.clear();
  }
}

function headersToMetadata(headers?: HeadersInit): { values: Record<string, string> } {
  const values: Record<string, string> = {};
  if (headers) {
    const h = new Headers(headers);
    h.forEach((value, key) => {
      values[key] = value;
    });
  }
  return { values };
}

function metadataToHeaders(metadata?: { values?: Record<string, string> }): Headers {
  const headers = new Headers();
  if (metadata?.values) {
    Object.entries(metadata.values).forEach(([key, value]) => {
      headers.set(key, value);
    });
  }
  return headers;
}

/** Maps RpcError code string (e.g. "NOT_FOUND") to Connect Code enum. Falls back to Code.Unknown. */
function rpcErrorToConnectError(err: RpcError): ConnectError {
  const normalized = err.code.toLowerCase().replace(/^cancelled$/, "canceled");
  const code = codeFromString(normalized) ?? Code.Unknown;
  return new ConnectError(err.message, code);
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
   * transport on the room — instead of installing its own listener and using the module-global
   * counter. Omit for a standalone transport.
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

  private get pendingUnary(): Map<number, PendingUnaryCall> {
    return this.registry.pendingUnary;
  }

  private get pendingStreams(): Map<number, PendingStreamCall> {
    return this.registry.pendingStreams;
  }

  private allocateRequestId(): number {
    return this.registry.allocateRequestId();
  }

  /**
   * Send `request` to this transport's target. Resolves once every frame has been handed to LiveKit,
   * and rejects when one could not be — publishing on a room that has gone away fails.
   */
  private async publishRequest(request: RpcRequest): Promise<void> {
    const payload = toBinary(RpcRequestSchema, request as any);
    this.meter?.record("out", payload.length);
    if (this.debug) {
      transportLog(
        `publish request_id=${request.requestId} bytes=${payload.length} target=${this.targetIdentity}`
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
   * Send `request`, failing the call registered under its request id if it never went out.
   *
   * A request that was not published has no answer coming, so its caller has to be told — and the
   * rejection has to be observed either way, or a failed publish surfaces as an unhandled rejection
   * with a call left hanging beside it.
   */
  private publishRequestOrFailCall(request: RpcRequest, requestId: number): void {
    void this.publishRequest(request).catch((error) => {
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

  async unary<I extends DescMessage, O extends DescMessage>(
    method: DescMethodUnary<I, O>,
    _signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: MessageInitShape<I>
  ): Promise<UnaryResponse<I, O>> {
    const requestId = this.allocateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";

    if (this.debug) {
      transportLog(`unary request_id=${requestId} ${service}/${methodName}`);
    }

    const inputMessage = create(method.input as any, input);
    const inputBytes = toBinary(method.input as any, inputMessage);

    const rpcRequest = create(RpcRequestSchema, {
      requestId,
      requestMessage: inputBytes,
      callMetadata: create(CallMetadataSchema, {
        service,
        method: methodName,
      }),
      metadata: headersToMetadata(header),
      endOfStream: true,
      abort: false,
      senderIdentity: this.room.localParticipant.identity,
      clientEpoch: this.registry.clientEpoch,
    });

    const responsePromise = new Promise<RpcResponse>((resolve, reject) => {
      this.pendingUnary.set(requestId, {
        call: { service, method: methodName },
        target: this.targetIdentity,
        resolve,
        reject,
      });
    });

    if (_signal?.aborted) {
      const err = new Error("cancelled");
      if (this.debug) {
        transportLog(`error request_id=${requestId} cancelled`);
      }
      this.pendingUnary.delete(requestId);
      throw err;
    }

    const onAbort = () => {
      const pending = this.pendingUnary.get(requestId);
      if (pending) {
        this.pendingUnary.delete(requestId);
        if (this.debug) {
          transportLog(`error request_id=${requestId} cancelled`);
        }
        pending.reject(new Error("cancelled"));
      }
    };
    _signal?.addEventListener("abort", onAbort, { once: true });

    // A caller-supplied deadline is the only thing that ever settles a request the peer never
    // answers. That is a real failure mode, not a theoretical one: a chunk-framed request whose
    // frames are dropped in transit leaves the peer's reassembler permanently incomplete, so no
    // response is ever produced and the call would otherwise hang forever with no error at all.
    // Callers that pass no timeout keep the previous (indefinite) behaviour.
    const deadlineTimer =
      timeoutMs === undefined
        ? undefined
        : setTimeout(() => {
            const pending = this.pendingUnary.get(requestId);
            if (!pending) return;
            this.pendingUnary.delete(requestId);
            if (this.debug) {
              transportLog(`error request_id=${requestId} deadline_exceeded after ${timeoutMs}ms`);
            }
            pending.reject(
              new ConnectError(
                `${service}/${methodName} did not respond within ${timeoutMs}ms`,
                Code.DeadlineExceeded
              )
            );
          }, timeoutMs);

    this.publishRequestOrFailCall(rpcRequest as any, requestId);

    try {
      const response = await responsePromise;
      if (deadlineTimer !== undefined) clearTimeout(deadlineTimer);
      _signal?.removeEventListener("abort", onAbort);

      if (response.error) {
        throw rpcErrorToConnectError(response.error);
      }

      // `responseMessage` is a non-optional proto3 `bytes` field — it decodes to an empty
      // `Uint8Array`, never `undefined`, whether the server genuinely sent zero bytes or omitted
      // the field entirely. A zero-length payload is exactly how protobuf serializes a message
      // whose every field is at its default (e.g. `ListSessionsResponse{ sessions: [] }`), so it
      // must decode as a normal successful response, not be rejected as "missing."
      const outputMessage = fromBinary(method.output as any, response.responseMessage);

      if (this.debug) {
        transportLog(`unary response request_id=${requestId} message=${safeStringify((outputMessage as any)?.message ?? outputMessage)}`);
      }

      return {
        stream: false,
        service: (method as any).parent,
        method,
        header: metadataToHeaders(response.metadata),
        message: outputMessage as any,
        trailer: metadataToHeaders(response.trailers),
      } as UnaryResponse<I, O>;
    } catch (e) {
      if (deadlineTimer !== undefined) clearTimeout(deadlineTimer);
      _signal?.removeEventListener("abort", onAbort);
      throw e;
    }
  }

  async stream<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>
  ): Promise<StreamResponse<I, O>> {
    const methodKind = (method as any).methodKind;

    if (methodKind === "client_streaming") {
      return this.handleClientStreaming(method as any, header, input, signal, timeoutMs);
    }
    if (methodKind === "server_streaming") {
      return this.handleServerStreaming(method as any, header, input, signal, timeoutMs);
    }
    if (methodKind === "bidi_streaming") {
      return this.handleBidiStreaming(method as any, header, input, signal, timeoutMs);
    }

    throw new Error(`Unknown method kind: ${methodKind}`);
  }

  /**
   * The two ways a streaming call can end without the peer: its caller's abort signal, and its
   * deadline. Registered against `requestId` before the request goes out, and released by the
   * registry when the call settles — whichever way it settles — so a finished call leaves neither a
   * live timer nor a listener behind.
   *
   * `aborted` reports a signal that had *already* fired, which is what an unmounted effect passes:
   * such a call is over before it is made, so nothing is watched and no request should go out.
   */
  private watchCallLifetime(
    requestId: number,
    call: PendingCall,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined
  ): { aborted: boolean; release: ReleaseCallWatchers } {
    if (signal?.aborted) {
      return { aborted: true, release: () => {} };
    }

    // TODO: cancellation is local only — the peer is not told, so it keeps serving a stream nobody
    // reads until it ends on its own. `RpcRequest.abort` is the field for saying so.
    const onAbort = () => {
      if (this.debug) {
        transportLog(`cancel request_id=${requestId} ${call.service}/${call.method}`);
      }
      this.registry.cancelCall(requestId);
    };
    signal?.addEventListener("abort", onAbort, { once: true });

    const deadlineTimer =
      timeoutMs === undefined
        ? undefined
        : setTimeout(() => {
            if (this.debug) {
              transportLog(`error request_id=${requestId} deadline_exceeded after ${timeoutMs}ms`);
            }
            this.registry.failCall(
              requestId,
              new ConnectError(
                `${call.service}/${call.method} did not respond within ${timeoutMs}ms`,
                Code.DeadlineExceeded
              )
            );
          }, timeoutMs);

    return {
      aborted: false,
      release: () => {
        signal?.removeEventListener("abort", onAbort);
        if (deadlineTimer !== undefined) clearTimeout(deadlineTimer);
      },
    };
  }

  /** The ConnectRPC response for a streaming call, decoding each frame the registry queues for it. */
  private streamResponseOf<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    responseQueue: AsyncQueue<Uint8Array>
  ): StreamResponse<I, O> {
    const asyncIterable = (async function* () {
      for await (const bytes of responseQueue) {
        yield fromBinary(method.output as any, bytes) as unknown as O;
      }
    })();

    return {
      stream: true,
      service: (method as any).parent,
      method,
      header: new Headers(),
      message: asyncIterable,
      trailer: new Headers(),
    } as StreamResponse<I, O>;
  }

  private async handleClientStreaming<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined
  ): Promise<StreamResponse<I, O>> {
    const requestId = this.allocateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";
    const call: PendingCall = { service, method: methodName };

    if (this.debug) {
      transportLog(`client_streaming request_id=${requestId} ${service}/${methodName}`);
    }

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      // Abandoned before its first message went out. Nothing is registered and nothing is sent; the
      // caller is told the call is cancelled, because a single-message response has no ending to
      // hand back in place of the message it never got.
      throw new ConnectError(`${service}/${methodName} was cancelled`, Code.Canceled);
    }

    const responsePromise = new Promise<RpcResponse>((resolve, reject) => {
      this.pendingUnary.set(requestId, {
        call,
        target: this.targetIdentity,
        resolve: resolve as any,
        reject,
        release: lifetime.release,
      });
    });

    let isFirst = true;
    for await (const item of input) {
      const inputMessage = create(method.input as any, item);
      const inputBytes = toBinary(method.input as any, inputMessage);
      const rpcRequest = create(RpcRequestSchema, {
        requestId,
        requestMessage: inputBytes,
        callMetadata: isFirst
          ? create(CallMetadataSchema, { service, method: methodName })
          : undefined,
        metadata: isFirst ? headersToMetadata(header) : undefined,
        endOfStream: false,
        abort: false,
        senderIdentity: this.room.localParticipant.identity,
      clientEpoch: this.registry.clientEpoch,
      });
      isFirst = false;
      this.publishRequestOrFailCall(rpcRequest as any, requestId);
    }

    const endRequest = create(RpcRequestSchema, {
      requestId,
      requestMessage: new Uint8Array(0),
      callMetadata: isFirst ? create(CallMetadataSchema, { service, method: methodName }) : undefined,
      metadata: isFirst ? headersToMetadata(header) : undefined,
      endOfStream: true,
      abort: false,
      senderIdentity: this.room.localParticipant.identity,
      clientEpoch: this.registry.clientEpoch,
    });
    this.publishRequestOrFailCall(endRequest as any, requestId);

    const response = await responsePromise;

    if (response.error) {
      throw rpcErrorToConnectError(response.error);
    }

    const outputMessage = fromBinary(method.output as any, response.responseMessage);

    const asyncIterable = (async function* () {
      yield outputMessage;
    })();

    return {
      stream: true,
      service: (method as any).parent,
      method,
      header: metadataToHeaders(response.metadata),
      message: asyncIterable as any,
      trailer: new Headers(),
    } as StreamResponse<I, O>;
  }

  private async handleServerStreaming<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined
  ): Promise<StreamResponse<I, O>> {
    const requestId = this.allocateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";
    const call: PendingCall = { service, method: methodName };

    if (this.debug) {
      transportLog(`server_streaming request_id=${requestId} ${service}/${methodName}`);
    }

    const responseQueue = new AsyncQueue<Uint8Array>();

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      return this.cancelledBeforeItWasSent(method, requestId, responseQueue);
    }

    this.pendingStreams.set(requestId, {
      call,
      target: this.targetIdentity,
      queue: responseQueue,
      release: lifetime.release,
    });

    let inputMessage: unknown = null;
    for await (const item of input) {
      inputMessage = create(method.input as any, item);
      break;
    }

    if (!inputMessage) {
      // No request can go out, so the registration made above — and the abort listener and deadline
      // timer with it — is retired before the caller is told.
      const noInput = new ConnectError(
        "Server streaming requires at least one input message",
        Code.Internal
      );
      this.registry.failCall(requestId, noInput);
      throw noInput;
    }

    const inputBytes = toBinary(method.input as any, inputMessage as any);
    const rpcRequest = create(RpcRequestSchema, {
      requestId,
      requestMessage: inputBytes,
      callMetadata: create(CallMetadataSchema, { service, method: methodName }),
      metadata: headersToMetadata(header),
      endOfStream: true,
      abort: false,
      senderIdentity: this.room.localParticipant.identity,
      clientEpoch: this.registry.clientEpoch,
    });
    this.publishRequestOrFailCall(rpcRequest as any, requestId);

    return this.streamResponseOf(method, responseQueue);
  }

  private async handleBidiStreaming<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined
  ): Promise<StreamResponse<I, O>> {
    const requestId = this.allocateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";
    const call: PendingCall = { service, method: methodName };

    if (this.debug) {
      transportLog(`bidi_streaming request_id=${requestId} ${service}/${methodName}`);
    }

    const responseQueue = new AsyncQueue<Uint8Array>();

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      return this.cancelledBeforeItWasSent(method, requestId, responseQueue);
    }

    this.pendingStreams.set(requestId, {
      call,
      target: this.targetIdentity,
      queue: responseQueue,
      release: lifetime.release,
    });

    const sendPromise = (async () => {
      let isFirst = true;
      for await (const item of input) {
        const inputMessage = create(method.input as any, item);
        const inputBytes = toBinary(method.input as any, inputMessage);
        const rpcRequest = create(RpcRequestSchema, {
          requestId,
          requestMessage: inputBytes,
          callMetadata: isFirst
            ? create(CallMetadataSchema, { service, method: methodName })
            : undefined,
          metadata: isFirst ? headersToMetadata(header) : undefined,
          endOfStream: false,
          abort: false,
          senderIdentity: this.room.localParticipant.identity,
      clientEpoch: this.registry.clientEpoch,
        });
        isFirst = false;
        this.publishRequestOrFailCall(rpcRequest as any, requestId);
      }
      const endRequest = create(RpcRequestSchema, {
        requestId,
        requestMessage: new Uint8Array(0),
        callMetadata: undefined,
        metadata: undefined,
        endOfStream: true,
        abort: false,
        senderIdentity: this.room.localParticipant.identity,
      clientEpoch: this.registry.clientEpoch,
      });
      this.publishRequestOrFailCall(endRequest as any, requestId);
    })();

    void sendPromise;

    return this.streamResponseOf(method, responseQueue);
  }

  /**
   * The response for a streaming call whose caller had already aborted when it was made — what an
   * unmounted effect's spent signal produces. Nothing was registered and nothing goes out; the
   * response ends, because a caller that cancelled its own call has nothing left to be told.
   */
  private cancelledBeforeItWasSent<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    requestId: number,
    responseQueue: AsyncQueue<Uint8Array>
  ): StreamResponse<I, O> {
    if (this.debug) {
      transportLog(`cancel request_id=${requestId} before it was sent`);
    }
    responseQueue.close();
    return this.streamResponseOf(method, responseQueue);
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

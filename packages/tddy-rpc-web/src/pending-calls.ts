/**
 * The calls a connection is waiting for answers to, and the rules for deciding which response
 * frame answers which of them.
 *
 * Correlation by request id alone is not enough. Ids restart at 1 whenever a page rebuilds its id
 * space, while the host may still be streaming for ids the new page is about to hand out again — so
 * a frame is delivered only when it names this connection's epoch *and* the method the id currently
 * holds. Delivered without those checks, a stale stream's bytes are decoded as another call's
 * message type with no error at all.
 */

import { fromBinary } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { codeFromString } from "@connectrpc/connect/protocol-connect";
import {
  RpcResponseSchema,
  type RpcError,
  type RpcResponse,
} from "./gen/rpc_envelope_pb.js";
import { AsyncQueue } from "./async-queue.js";

/**
 * Distinguishes one client connection from the next.
 *
 * A request id restarts at 1 whenever a page builds a fresh connection, while the host may keep
 * publishing frames of streams the *previous* page opened, tagged with ids the new page is about to
 * hand out again. Without a per-connection discriminator those frames resolve whichever call now
 * holds the id, and their bytes are decoded as that call's message type. Never 0: a zero epoch on
 * the wire means the field was absent.
 */
export function mintClientEpoch(): number {
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
export type ReleaseCallWatchers = () => void;

/** What every pending entry carries, whatever shape its response takes. */
interface PendingCallEntry {
  call: PendingCall;
  /**
   * The peer this call was sent to. Only meaningful when one correlation state serves several peers
   * — a LiveKit room's transports share one id space across every daemon in the room, so a peer's
   * departure can only be turned into the right failures by target. A pipe with a single peer at the
   * other end has nothing to distinguish and leaves it unset.
   */
  target?: string;
  release?: ReleaseCallWatchers;
}

export interface PendingUnaryCall extends PendingCallEntry {
  resolve: (value: RpcResponse) => void;
  reject: (err: Error) => void;
}

export interface PendingStreamCall extends PendingCallEntry {
  queue: AsyncQueue<Uint8Array>;
}

/** Maps RpcError code string (e.g. "NOT_FOUND") to Connect Code enum. Falls back to Code.Unknown. */
export function rpcErrorToConnectError(err: RpcError): ConnectError {
  const normalized = err.code.toLowerCase().replace(/^cancelled$/, "canceled");
  const code = codeFromString(normalized) ?? Code.Unknown;
  return new ConnectError(err.message, code);
}

/**
 * One connection's request-id space and the calls registered in it.
 *
 * A connection holds exactly one of these, however many transports it vends: a response id then
 * maps to exactly one call regardless of which peer replied, and there is one implementation of
 * "does this response answer this call" that a new dispatch path cannot forget to ask.
 */
export class PendingCalls {
  readonly pendingUnary = new Map<number, PendingUnaryCall>();
  readonly pendingStreams = new Map<number, PendingStreamCall>();
  private nextId = 1;

  constructor(
    /** This connection's identity, stamped on every request and required on every response. */
    readonly clientEpoch: number = mintClientEpoch(),
    /** Receives a line per refused frame. Omitted, refusals are silent. */
    private readonly log?: (message: string) => void,
  ) {}

  allocateRequestId(): number {
    return this.nextId++;
  }

  /**
   * Settle the call registered under `requestId` as a failure — a deadline, a departed peer, a
   * request that never went out — and release its registration. A no-op once the call has settled,
   * so a deadline that fires after its response arrived costs nothing.
   */
  failCall(requestId: number, error: Error): void {
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

  /** The ids of the calls in flight whose registration satisfies `predicate`. */
  requestIdsMatching(predicate: (pending: PendingUnaryCall | PendingStreamCall) => boolean): number[] {
    const matching: number[] = [];
    for (const [requestId, pending] of this.pendingStreams) {
      if (predicate(pending)) matching.push(requestId);
    }
    for (const [requestId, pending] of this.pendingUnary) {
      if (predicate(pending)) matching.push(requestId);
    }
    return matching;
  }

  /** The ids of every call in flight. */
  requestIdsInFlight(): number[] {
    return [...this.pendingStreams.keys(), ...this.pendingUnary.keys()];
  }

  /**
   * Decode one response frame and hand it to the call it answers.
   *
   * Throws when `frame` does not decode as an `RpcResponse` — a decode failure is how a framing bug
   * reaches the app, so the caller surfaces it rather than this dropping it silently.
   */
  deliverFrame(frame: Uint8Array): void {
    this.deliver(fromBinary(RpcResponseSchema, frame) as RpcResponse);
  }

  /**
   * Hand `response` to the call it answers: a stream frame is queued (and the stream closed or
   * failed when the frame says so), a single-message response resolves its call. A frame that
   * answers no call of this connection is dropped.
   */
  deliver(response: RpcResponse): void {
    if (!this.answersItsCall(response)) return;
    const requestId = response.requestId;
    const pendingStream = this.pendingStreams.get(requestId);
    if (pendingStream) {
      const streamQueue = pendingStream.queue;
      if (response.error) {
        this.retireStream(requestId);
        streamQueue.fail(rpcErrorToConnectError(response.error));
        return;
      }
      if (response.responseMessage && response.responseMessage.length > 0) {
        streamQueue.enqueue(response.responseMessage);
      }
      if (response.endOfStream) {
        this.retireStream(requestId);
        streamQueue.close();
      }
      return;
    }
    this.retireUnary(requestId)?.resolve(response);
  }

  /**
   * Whether `response` answers the call currently registered under its request id.
   *
   * A matching id is not enough. The host keeps serving streams opened by a connection that has gone
   * away and addresses them to the same peer, while ids restart from 1 — so an id match alone can
   * mean "a dead page's stream", and delivering it hands the caller another call's bytes to decode
   * as its own message type, with no error. That is how a terminal's output came to be rendered as a
   * control lease's holder screen id.
   */
  private answersItsCall(response: RpcResponse): boolean {
    if (response.clientEpoch !== this.clientEpoch) {
      this.log?.(
        `dropping response request_id=${response.requestId} from client_epoch=${response.clientEpoch} (this connection is ${this.clientEpoch})`,
      );
      return false;
    }
    const pending =
      this.pendingStreams.get(response.requestId) ?? this.pendingUnary.get(response.requestId);
    const answered = response.callMetadata;
    if (!pending || !answered) return true;
    if (pending.call.service === answered.service && pending.call.method === answered.method) {
      return true;
    }
    this.log?.(
      `dropping response request_id=${response.requestId} answering ${answered.service}/${answered.method}, but that id holds ${pending.call.service}/${pending.call.method}`,
    );
    return false;
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
}

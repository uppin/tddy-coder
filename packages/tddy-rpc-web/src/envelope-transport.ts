/**
 * ConnectRPC `Transport` over the `tddy-rpc` envelope, on any byte pipe.
 *
 * Everything here is transport-independent: request-id allocation, the per-connection client
 * epoch that stops a previous page's streams from resolving this page's calls, refusing a response
 * that answers a different method than the call holding its id, pending-call settlement, and
 * turning a sequence of response frames into an async iterable. A concrete flavour supplies only a
 * {@link FramePipe} — a LiveKit data channel, a webview IPC bridge, whatever comes next.
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
import {
  RpcRequestSchema,
  CallMetadataSchema,
  type RpcRequest,
  type RpcResponse,
} from "./gen/rpc_envelope_pb.js";
import { AsyncQueue } from "./async-queue.js";
import {
  PendingCalls,
  mintClientEpoch,
  rpcErrorToConnectError,
  type PendingCall,
  type ReleaseCallWatchers,
} from "./pending-calls.js";

/** Receives frames and closure from a {@link FramePipe}. */
export interface FrameListener {
  /** One encoded `RpcResponse` frame arrived from the host. */
  onFrame(frame: Uint8Array): void;
  /** The pipe is permanently gone; every pending call must settle. */
  onClose(reason: string): void;
}

/** A duplex byte pipe carrying encoded envelope frames in both directions. */
export interface FramePipe {
  /** Send one encoded `RpcRequest` frame to the host. */
  send(frame: Uint8Array): void;
  /** Start receiving frames. Returns a function that stops receiving them. */
  subscribe(listener: FrameListener): () => void;
}

export interface EnvelopeTransportOptions {
  pipe: FramePipe;
  /**
   * Distinguishes this connection from the ones before it. A request id restarts at 1 whenever a
   * page rebuilds its id space, while the host may still be streaming for ids the new page is
   * about to hand out again. Minted per connection when omitted.
   */
  clientEpoch?: number;
  /** Names the peer in error messages. */
  label?: string;
}

/** What an {@link EnvelopeTransport} needs besides the envelope rules themselves. */
export interface EnvelopeCallOptions {
  /** The connection's id space and the calls registered in it — shared by every transport on it. */
  calls: PendingCalls;
  /**
   * Hand one encoded `RpcRequest` frame to the pipe. A frame that cannot be sent has no answer
   * coming, so an implementation that discovers a send failure must settle `requestId` — see
   * {@link PendingCalls.failCall}.
   */
  sendFrame(frame: Uint8Array, requestId: number): void;
  /**
   * Stamped on every request frame, where the pipe gives this endpoint a name of its own. Resolved
   * per frame: a name that comes from a connection is not known until that connection is up, which
   * can be after this transport was built.
   */
  senderIdentity?: () => string | undefined;
  /** Recorded on every call this transport opens — see the `target` of a pending entry. */
  target?: string;
  /** Names the peer in error messages. */
  label?: string;
  /** Receives a line per call event. Omitted, calls are not logged. */
  log?: (message: string) => void;
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

/**
 * A ConnectRPC transport that turns calls into `tddy-rpc` envelope frames and the frames that come
 * back into responses. It sends through {@link EnvelopeCallOptions.sendFrame} and is answered
 * through {@link EnvelopeCallOptions.calls}, so it knows nothing about the pipe underneath.
 */
export class EnvelopeTransport implements Transport {
  private readonly calls: PendingCalls;

  constructor(private readonly options: EnvelopeCallOptions) {
    this.calls = options.calls;
  }

  async unary<I extends DescMessage, O extends DescMessage>(
    method: DescMethodUnary<I, O>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: MessageInitShape<I>,
  ): Promise<UnaryResponse<I, O>> {
    const requestId = this.calls.allocateRequestId();
    const call = this.callOf(method);
    this.options.log?.(`unary request_id=${requestId} ${call.service}/${call.method}`);

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      // Abandoned before its request went out. Nothing is registered and nothing is sent; the
      // caller is told the call is cancelled, because a single-message response has no ending to
      // hand back in place of the message it never got.
      throw new ConnectError(`${call.service}/${call.method} was cancelled`, Code.Canceled);
    }

    const responsePromise = new Promise<RpcResponse>((resolve, reject) => {
      this.calls.pendingUnary.set(requestId, {
        call,
        target: this.options.target,
        resolve,
        reject,
        release: lifetime.release,
      });
    });

    this.sendRequest(
      this.requestFrame({
        requestId,
        call,
        header,
        message: this.encodeInput(method, input),
        endOfStream: true,
      }),
      requestId,
    );

    const response = await responsePromise;

    if (response.error) {
      throw rpcErrorToConnectError(response.error);
    }

    // `responseMessage` is a non-optional proto3 `bytes` field — it decodes to an empty
    // `Uint8Array`, never `undefined`, whether the server genuinely sent zero bytes or omitted
    // the field entirely. A zero-length payload is exactly how protobuf serializes a message
    // whose every field is at its default (e.g. `ListSessionsResponse{ sessions: [] }`), so it
    // must decode as a normal successful response, not be rejected as "missing."
    const outputMessage = fromBinary(method.output as any, response.responseMessage);

    this.options.log?.(
      `unary response request_id=${requestId} message=${safeStringify(
        (outputMessage as any)?.message ?? outputMessage,
      )}`,
    );

    return {
      stream: false,
      service: (method as any).parent,
      method,
      header: metadataToHeaders(response.metadata),
      message: outputMessage as any,
      trailer: metadataToHeaders(response.trailers),
    } as UnaryResponse<I, O>;
  }

  async stream<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
  ): Promise<StreamResponse<I, O>> {
    const methodKind = (method as any).methodKind;

    if (methodKind === "client_streaming") {
      return this.handleClientStreaming(method, header, input, signal, timeoutMs);
    }
    if (methodKind === "server_streaming") {
      return this.handleServerStreaming(method, header, input, signal, timeoutMs);
    }
    if (methodKind === "bidi_streaming") {
      return this.handleBidiStreaming(method, header, input, signal, timeoutMs);
    }

    throw new Error(`Unknown method kind: ${methodKind}`);
  }

  /** The service and method a descriptor names, as a response frame reports them. */
  private callOf(method: DescMethodUnary | DescMethodStreaming): PendingCall {
    return {
      service: (method as any).parent?.typeName ?? "unknown",
      method: (method as any).name ?? "unknown",
    };
  }

  private encodeInput<I extends DescMessage>(
    method: { input: I },
    input: MessageInitShape<I>,
  ): Uint8Array {
    return toBinary(method.input as any, create(method.input as any, input));
  }

  /**
   * One outbound request frame. `callMetadata` and `metadata` name the call being opened, so they
   * ride only on the frame that opens it — a later frame of the same client stream carries neither.
   */
  private requestFrame(frame: {
    requestId: number;
    call?: PendingCall;
    header?: HeadersInit;
    message: Uint8Array;
    endOfStream: boolean;
  }): RpcRequest {
    return create(RpcRequestSchema, {
      requestId: frame.requestId,
      requestMessage: frame.message,
      callMetadata: frame.call
        ? create(CallMetadataSchema, { service: frame.call.service, method: frame.call.method })
        : undefined,
      metadata: frame.call ? headersToMetadata(frame.header) : undefined,
      endOfStream: frame.endOfStream,
      abort: false,
      senderIdentity: this.options.senderIdentity?.(),
      clientEpoch: this.calls.clientEpoch,
    }) as RpcRequest;
  }

  /** Encode `request` and hand it to the pipe, failing the call under `requestId` if it cannot go. */
  private sendRequest(request: RpcRequest, requestId: number): void {
    this.options.sendFrame(toBinary(RpcRequestSchema, request as any), requestId);
  }

  /**
   * The two ways a call can end without the peer: its caller's abort signal, and its deadline.
   * Registered against `requestId` before the request goes out, and released by
   * {@link PendingCalls} when the call settles — whichever way it settles — so a finished call
   * leaves neither a live timer nor a listener behind.
   *
   * `aborted` reports a signal that had *already* fired, which is what an unmounted effect passes:
   * such a call is over before it is made, so nothing is watched and no request should go out.
   */
  private watchCallLifetime(
    requestId: number,
    call: PendingCall,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
  ): { aborted: boolean; release: ReleaseCallWatchers } {
    if (signal?.aborted) {
      return { aborted: true, release: () => {} };
    }

    // TODO: cancellation is local only — the peer is not told, so it keeps serving a stream nobody
    // reads until it ends on its own. `RpcRequest.abort` is the field for saying so.
    const onAbort = () => {
      this.options.log?.(`cancel request_id=${requestId} ${call.service}/${call.method}`);
      this.calls.cancelCall(requestId);
    };
    signal?.addEventListener("abort", onAbort, { once: true });

    // A caller-supplied deadline is the only thing that ever settles a request the peer never
    // answers. That is a real failure mode, not a theoretical one: a request whose frames are
    // dropped in transit leaves the peer's reassembler permanently incomplete, so no response is
    // ever produced and the call would otherwise hang forever with no error at all. Callers that
    // pass no timeout wait indefinitely.
    const deadlineTimer =
      timeoutMs === undefined
        ? undefined
        : setTimeout(() => {
            this.options.log?.(
              `error request_id=${requestId} deadline_exceeded after ${timeoutMs}ms`,
            );
            this.calls.failCall(
              requestId,
              new ConnectError(
                `${call.service}/${call.method} did not respond within ${timeoutMs}ms`,
                Code.DeadlineExceeded,
              ),
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

  /** The ConnectRPC response for a streaming call, decoding each frame queued for it. */
  private streamResponseOf<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    responseQueue: AsyncQueue<Uint8Array>,
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
    timeoutMs: number | undefined,
  ): Promise<StreamResponse<I, O>> {
    const requestId = this.calls.allocateRequestId();
    const call = this.callOf(method);
    this.options.log?.(`client_streaming request_id=${requestId} ${call.service}/${call.method}`);

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      // Abandoned before its first message went out. Nothing is registered and nothing is sent; the
      // caller is told the call is cancelled, because a single-message response has no ending to
      // hand back in place of the message it never got.
      throw new ConnectError(`${call.service}/${call.method} was cancelled`, Code.Canceled);
    }

    const responsePromise = new Promise<RpcResponse>((resolve, reject) => {
      this.calls.pendingUnary.set(requestId, {
        call,
        target: this.options.target,
        resolve,
        reject,
        release: lifetime.release,
      });
    });

    let isFirst = true;
    for await (const item of input) {
      this.sendRequest(
        this.requestFrame({
          requestId,
          call: isFirst ? call : undefined,
          header,
          message: this.encodeInput(method, item),
          endOfStream: false,
        }),
        requestId,
      );
      isFirst = false;
    }

    this.sendRequest(
      this.requestFrame({
        requestId,
        call: isFirst ? call : undefined,
        header,
        message: new Uint8Array(0),
        endOfStream: true,
      }),
      requestId,
    );

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
    timeoutMs: number | undefined,
  ): Promise<StreamResponse<I, O>> {
    const requestId = this.calls.allocateRequestId();
    const call = this.callOf(method);
    this.options.log?.(`server_streaming request_id=${requestId} ${call.service}/${call.method}`);

    const responseQueue = new AsyncQueue<Uint8Array>();

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      return this.cancelledBeforeItWasSent(method, requestId, responseQueue);
    }

    this.calls.pendingStreams.set(requestId, {
      call,
      target: this.options.target,
      queue: responseQueue,
      release: lifetime.release,
    });

    let firstInput: MessageInitShape<I> | undefined;
    let hasInput = false;
    for await (const item of input) {
      firstInput = item;
      hasInput = true;
      break;
    }

    if (!hasInput) {
      // No request can go out, so the registration made above — and the abort listener and deadline
      // timer with it — is retired before the caller is told.
      const noInput = new ConnectError(
        "Server streaming requires at least one input message",
        Code.Internal,
      );
      this.calls.failCall(requestId, noInput);
      throw noInput;
    }

    this.sendRequest(
      this.requestFrame({
        requestId,
        call,
        header,
        message: this.encodeInput(method, firstInput as MessageInitShape<I>),
        endOfStream: true,
      }),
      requestId,
    );

    return this.streamResponseOf(method, responseQueue);
  }

  private async handleBidiStreaming<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
    signal: AbortSignal | undefined,
    timeoutMs: number | undefined,
  ): Promise<StreamResponse<I, O>> {
    const requestId = this.calls.allocateRequestId();
    const call = this.callOf(method);
    this.options.log?.(`bidi_streaming request_id=${requestId} ${call.service}/${call.method}`);

    const responseQueue = new AsyncQueue<Uint8Array>();

    const lifetime = this.watchCallLifetime(requestId, call, signal, timeoutMs);
    if (lifetime.aborted) {
      return this.cancelledBeforeItWasSent(method, requestId, responseQueue);
    }

    this.calls.pendingStreams.set(requestId, {
      call,
      target: this.options.target,
      queue: responseQueue,
      release: lifetime.release,
    });

    // The request messages are pumped for as long as the caller produces them, alongside the
    // responses the queue is already delivering — a bidi call's two directions do not take turns.
    void (async () => {
      let isFirst = true;
      for await (const item of input) {
        this.sendRequest(
          this.requestFrame({
            requestId,
            call: isFirst ? call : undefined,
            header,
            message: this.encodeInput(method, item),
            endOfStream: false,
          }),
          requestId,
        );
        isFirst = false;
      }
      this.sendRequest(
        this.requestFrame({
          requestId,
          message: new Uint8Array(0),
          endOfStream: true,
        }),
        requestId,
      );
    })();

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
    responseQueue: AsyncQueue<Uint8Array>,
  ): StreamResponse<I, O> {
    this.options.log?.(`cancel request_id=${requestId} before it was sent`);
    responseQueue.close();
    return this.streamResponseOf(method, responseQueue);
  }
}

/** A ConnectRPC transport that speaks the `tddy-rpc` envelope over `options.pipe`. */
export function createEnvelopeTransport(options: EnvelopeTransportOptions): Transport {
  const label = options.label ?? "the host";
  const calls = new PendingCalls(options.clientEpoch ?? mintClientEpoch());
  const pipe = options.pipe;

  /** Set once the pipe hands it back — a pipe that closes during `subscribe` has nothing to stop. */
  const subscription: { stop?: () => void } = {};
  subscription.stop = pipe.subscribe({
    onFrame(frame: Uint8Array) {
      try {
        calls.deliverFrame(frame);
      } catch (error) {
        // A frame that cannot be turned into a response is a real fault — a framing bug reaches the
        // app as a call that never settles — so it is surfaced rather than dropped in silence.
        console.error(`[tddy-rpc] could not deliver a response frame from ${label}:`, error);
      }
    },
    onClose(reason: string) {
      subscription.stop?.();
      // Correlation is gone: no later frame can settle these calls, so every one of them is failed
      // rather than left waiting for an answer nobody is there to send.
      for (const requestId of calls.requestIdsInFlight()) {
        calls.failCall(requestId, new ConnectError(reason, Code.Unavailable));
      }
    },
  });

  return new EnvelopeTransport({
    calls,
    label,
    sendFrame(frame, requestId) {
      try {
        pipe.send(frame);
      } catch (error) {
        calls.failCall(
          requestId,
          new ConnectError(
            `could not send request to ${label}: ${ConnectError.from(error).rawMessage}`,
            Code.Unavailable,
          ),
        );
      }
    },
  });
}

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
import {
  RpcRequestSchema,
  RpcResponseSchema,
  CallMetadataSchema,
  type RpcRequest,
  type RpcResponse,
  type RpcError,
} from "./gen/rpc_envelope_pb.js";
import { AsyncQueue } from "./async-queue.js";

const RPC_TOPIC = "tddy-rpc";

let nextRequestId = 1;

function generateRequestId(): number {
  return nextRequestId++;
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
}

export class LiveKitTransport implements Transport {
  private room: Room;
  private targetIdentity: string;
  private debug: boolean;
  private pendingUnary = new Map<
    number,
    { resolve: (value: RpcResponse) => void; reject: (err: Error) => void }
  >();
  private pendingStreams = new Map<number, AsyncQueue<Uint8Array>>();
  private listener: ((payload: Uint8Array, participant?: { identity: string }, topic?: string) => void) | null = null;

  constructor(options: LiveKitTransportOptions) {
    this.room = options.room;
    this.targetIdentity = options.targetIdentity;
    this.debug = options.debug ?? false;

    this.listener = (
      payload: Uint8Array,
      participant?: { identity?: string } | null,
      _kind?: unknown,
      topic?: string
    ) => {
      if (topic !== RPC_TOPIC) return;
      if (participant != null && participant.identity != null && participant.identity !== this.targetIdentity) return;

      try {
        const response = fromBinary(RpcResponseSchema, payload) as RpcResponse;
        const requestId = response.requestId;

        if (this.debug) {
          console.log(
            `[LiveKitTransport] response request_id=${requestId} endOfStream=${response.endOfStream} error=${response.error ? response.error.message : "none"}`
          );
        }

        const streamQueue = this.pendingStreams.get(requestId);
        if (streamQueue) {
          if (response.error) {
            streamQueue.fail(rpcErrorToConnectError(response.error));
            this.pendingStreams.delete(requestId);
          } else {
            if (response.responseMessage && response.responseMessage.length > 0) {
              streamQueue.enqueue(response.responseMessage);
            }
            if (response.endOfStream) {
              streamQueue.close();
              this.pendingStreams.delete(requestId);
            }
          }
        } else {
          const pending = this.pendingUnary.get(requestId);
          if (pending) {
            this.pendingUnary.delete(requestId);
            pending.resolve(response);
          }
        }
      } catch (e) {
        if (this.debug) {
          console.log(`[LiveKitTransport] decode error:`, e);
        }
      }
    };

    this.room.on(RoomEvent.DataReceived, this.listener as any);
    if (this.debug) {
      console.log(
        `[LiveKitTransport] created, listening for DataReceived topic=${RPC_TOPIC} target=${this.targetIdentity}`
      );
    }
  }

  private publishRequest(request: RpcRequest): void {
    const payload = toBinary(RpcRequestSchema, request as any);
    if (this.debug) {
      console.log(
        `[LiveKitTransport] publish request_id=${request.requestId} bytes=${payload.length} target=${this.targetIdentity}`
      );
    }
    this.room.localParticipant.publishData(payload, {
      reliable: true,
      topic: RPC_TOPIC,
      destinationIdentities: [this.targetIdentity],
    });
  }

  async unary<I extends DescMessage, O extends DescMessage>(
    method: DescMethodUnary<I, O>,
    _signal: AbortSignal | undefined,
    _timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: MessageInitShape<I>
  ): Promise<UnaryResponse<I, O>> {
    const requestId = generateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";

    if (this.debug) {
      console.log(`[LiveKitTransport] unary request_id=${requestId} ${service}/${methodName}`);
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
    });

    const responsePromise = new Promise<RpcResponse>((resolve, reject) => {
      this.pendingUnary.set(requestId, { resolve, reject });
    });

    if (_signal?.aborted) {
      const err = new Error("cancelled");
      if (this.debug) {
        console.log(`[LiveKitTransport] error request_id=${requestId} cancelled`);
      }
      this.pendingUnary.delete(requestId);
      throw err;
    }

    const onAbort = () => {
      const pending = this.pendingUnary.get(requestId);
      if (pending) {
        this.pendingUnary.delete(requestId);
        if (this.debug) {
          console.log(`[LiveKitTransport] error request_id=${requestId} cancelled`);
        }
        pending.reject(new Error("cancelled"));
      }
    };
    _signal?.addEventListener("abort", onAbort, { once: true });

    this.publishRequest(rpcRequest as any);

    try {
      const response = await responsePromise;
      _signal?.removeEventListener("abort", onAbort);

      if (response.error) {
        throw rpcErrorToConnectError(response.error);
      }

      if (!response.responseMessage || response.responseMessage.length === 0) {
        throw new Error(`[LiveKitTransport] No response message for request_id=${requestId}`);
      }

      const outputMessage = fromBinary(method.output as any, response.responseMessage);

      if (this.debug) {
        console.log(`[LiveKitTransport] unary response request_id=${requestId} message=${JSON.stringify((outputMessage as any)?.message ?? outputMessage)}`);
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
      _signal?.removeEventListener("abort", onAbort);
      throw e;
    }
  }

  async stream<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    signal: AbortSignal | undefined,
    _timeoutMs: number | undefined,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>
  ): Promise<StreamResponse<I, O>> {
    const methodKind = (method as any).methodKind;
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";

    if (methodKind === "client_streaming") {
      return this.handleClientStreaming(method as any, header, input, signal);
    }
    if (methodKind === "server_streaming") {
      return this.handleServerStreaming(method as any, header, input, signal);
    }
    if (methodKind === "bidi_streaming") {
      return this.handleBidiStreaming(method as any, header, input, signal);
    }

    throw new Error(`Unknown method kind: ${methodKind}`);
  }

  private async handleClientStreaming<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
    _signal: AbortSignal | undefined
  ): Promise<StreamResponse<I, O>> {
    const requestId = generateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";

    if (this.debug) {
      console.log(`[LiveKitTransport] client_streaming request_id=${requestId} ${service}/${methodName}`);
    }

    const responsePromise = new Promise<RpcResponse>((resolve, reject) => {
      this.pendingUnary.set(requestId, {
        resolve: resolve as any,
        reject,
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
      });
      isFirst = false;
      this.publishRequest(rpcRequest as any);
    }

    const endRequest = create(RpcRequestSchema, {
      requestId,
      requestMessage: new Uint8Array(0),
      callMetadata: isFirst ? create(CallMetadataSchema, { service, method: methodName }) : undefined,
      metadata: isFirst ? headersToMetadata(header) : undefined,
      endOfStream: true,
      abort: false,
      senderIdentity: this.room.localParticipant.identity,
    });
    this.publishRequest(endRequest as any);

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
    _signal: AbortSignal | undefined
  ): Promise<StreamResponse<I, O>> {
    const requestId = generateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";

    if (this.debug) {
      console.log(`[LiveKitTransport] server_streaming request_id=${requestId} ${service}/${methodName}`);
    }

    const responseQueue = new AsyncQueue<Uint8Array>();

    this.pendingStreams.set(requestId, responseQueue);

    let inputMessage: unknown = null;
    for await (const item of input) {
      inputMessage = create(method.input as any, item);
      break;
    }

    if (!inputMessage) {
      throw new Error("Server streaming requires at least one input message");
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
    });
    this.publishRequest(rpcRequest as any);

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

  private async handleBidiStreaming<I extends DescMessage, O extends DescMessage>(
    method: DescMethodStreaming<I, O>,
    header: HeadersInit | undefined,
    input: AsyncIterable<MessageInitShape<I>>,
    _signal: AbortSignal | undefined
  ): Promise<StreamResponse<I, O>> {
    const requestId = generateRequestId();
    const service = (method as any).parent?.typeName ?? "unknown";
    const methodName = (method as any).name ?? "unknown";

    if (this.debug) {
      console.log(`[LiveKitTransport] bidi_streaming request_id=${requestId} ${service}/${methodName}`);
    }

    const responseQueue = new AsyncQueue<Uint8Array>();
    this.pendingStreams.set(requestId, responseQueue);

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
        });
        isFirst = false;
        this.publishRequest(rpcRequest as any);
      }
      const endRequest = create(RpcRequestSchema, {
        requestId,
        requestMessage: new Uint8Array(0),
        callMetadata: undefined,
        metadata: undefined,
        endOfStream: true,
        abort: false,
        senderIdentity: this.room.localParticipant.identity,
      });
      this.publishRequest(endRequest as any);
    })();

    const asyncIterable = (async function* () {
      for await (const bytes of responseQueue) {
        yield fromBinary(method.output as any, bytes) as unknown as O;
      }
    })();

    void sendPromise;

    return {
      stream: true,
      service: (method as any).parent,
      method,
      header: new Headers(),
      message: asyncIterable,
      trailer: new Headers(),
    } as StreamResponse<I, O>;
  }

  destroy(): void {
    if (this.listener) {
      this.room.off(RoomEvent.DataReceived, this.listener as any);
      this.listener = null;
    }
    this.pendingUnary.clear();
    this.pendingStreams.clear();
  }
}

export function createLiveKitTransport(options: LiveKitTransportOptions): Transport {
  return new LiveKitTransport(options);
}

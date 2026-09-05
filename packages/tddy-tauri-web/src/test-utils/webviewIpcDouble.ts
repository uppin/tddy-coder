/**
 * An in-memory {@link WebviewIpcBridge} standing in for the host application's IPC surface: it
 * records the request frames the page sends, and lets a test answer them, stream to them, fail
 * them, or close the channel underneath them.
 */

import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import {
  mintClientEpoch,
  RpcRequestSchema,
  RpcResponseSchema,
  type RpcRequest,
} from "tddy-rpc-web";
import { EchoResponseSchema } from "tddy-rpc-web/test-fixtures";
import type { WebviewIpcBridge } from "../transport.js";

export interface WebviewIpcDouble extends WebviewIpcBridge {
  /** The next request frame the page sends, awaited rather than polled. */
  nextRequest(): Promise<RpcRequest>;
  /** The epoch the page registered its response channel with, or 0 while it has registered none. */
  connectedEpoch(): number;
  /** Answer `request` with one terminal success frame. */
  answer(request: RpcRequest, message: string): void;
  /** Send `messages` as stream frames, then the closing frame. */
  stream(request: RpcRequest, messages: string[]): void;
  /** Send `messages` as stream frames and stop, leaving the call open. */
  streamPartially(request: RpcRequest, messages: string[]): void;
  /** Answer `request` with an error frame. */
  fail(request: RpcRequest, code: string, message: string): void;
  /** Push an arbitrary response frame — for frames a well-behaved host would not send. */
  respond(response: MessageInitShape<typeof RpcResponseSchema>): void;
  /** Report the channel permanently gone. */
  closeChannel(reason: string): void;
}

/**
 * The reason a released connection reports, word for word as the real bridge reports it, so a test
 * asserting on what the caller is told is asserting on what the host application would tell it.
 */
export const PAGE_RELEASED_THE_CONNECTION = "the page released this connection";

/** What the double stands for, unless a test names the connection it wants. */
export interface WebviewIpcDoubleOptions {
  /** The connection this bridge is, as the real one mints for itself. */
  clientEpoch?: number;
}

export function aWebviewIpcDouble(options: WebviewIpcDoubleOptions = {}): WebviewIpcDouble {
  // A real bridge mints its own epoch, so this one does too unless the test names it — a test that
  // asserts on the epoch its frames carry needs to know which number to expect.
  const clientEpoch = options.clientEpoch ?? mintClientEpoch();
  const arrived: RpcRequest[] = [];
  const waiting: Array<(request: RpcRequest) => void> = [];
  let onFrame: ((frame: Uint8Array) => void) | null = null;
  // 0 until a channel is registered: never a real epoch, so "connected" and "never connected" are
  // told apart rather than conflated by handing back the epoch this double was built with.
  let registeredEpoch = 0;
  let reportClosed: (reason: string) => void = () => {};
  const closed = new Promise<string>((resolve) => {
    reportClosed = resolve;
  });
  let released = false;

  const push = (response: MessageInitShape<typeof RpcResponseSchema>) => {
    const frame = toBinary(RpcResponseSchema, create(RpcResponseSchema, response));
    onFrame?.(frame);
  };

  const streamFrame = (request: RpcRequest, message: string) => ({
    requestId: request.requestId,
    clientEpoch: request.clientEpoch,
    callMetadata: request.callMetadata,
    responseMessage: anEchoResponseBody(message),
    endOfStream: false,
  });

  return {
    clientEpoch,
    async connect(handler) {
      onFrame = handler;
      registeredEpoch = clientEpoch;
    },
    async send(frame) {
      const request = fromBinary(RpcRequestSchema, frame);
      const next = waiting.shift();
      if (next) {
        next(request);
        return;
      }
      arrived.push(request);
    },
    closed,
    // Idempotent, as the real bridge is.
    async close(): Promise<void> {
      if (released) return;
      released = true;
      // Resolving `closed` is what a release means to everything above the bridge: it is how a
      // transport learns its peer is gone and settles the calls still waiting on one. A double that
      // only recorded the release would let a test pass while those calls hung in production.
      reportClosed(PAGE_RELEASED_THE_CONNECTION);
    },
    nextRequest() {
      const ready = arrived.shift();
      if (ready) {
        return Promise.resolve(ready);
      }
      return new Promise<RpcRequest>((resolve) => waiting.push(resolve));
    },
    connectedEpoch() {
      return registeredEpoch;
    },
    answer(request, message) {
      push({
        requestId: request.requestId,
        clientEpoch: request.clientEpoch,
        callMetadata: request.callMetadata,
        responseMessage: anEchoResponseBody(message),
        endOfStream: true,
      });
    },
    stream(request, messages) {
      for (const message of messages) {
        push(streamFrame(request, message));
      }
      push({
        requestId: request.requestId,
        clientEpoch: request.clientEpoch,
        callMetadata: request.callMetadata,
        endOfStream: true,
      });
    },
    streamPartially(request, messages) {
      for (const message of messages) {
        push(streamFrame(request, message));
      }
    },
    fail(request, code, message) {
      push({
        requestId: request.requestId,
        clientEpoch: request.clientEpoch,
        callMetadata: request.callMetadata,
        error: { code, message },
        endOfStream: true,
      });
    },
    respond: push,
    closeChannel(reason) {
      onFrame = null;
      reportClosed(reason);
    },
  };
}

/** An `EchoResponse` body, encoded the way the host would put it on the wire. */
export function anEchoResponseBody(message: string): Uint8Array {
  return toBinary(EchoResponseSchema, create(EchoResponseSchema, { message }));
}

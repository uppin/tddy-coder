/**
 * An in-memory {@link WebviewIpcBridge} standing in for the host application's IPC surface: it
 * records the request frames the page sends, and lets a test answer them, stream to them, fail
 * them, or close the channel underneath them.
 */

import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import {
  RpcRequestSchema,
  RpcResponseSchema,
  type RpcRequest,
} from "tddy-rpc-web";
import { EchoResponseSchema } from "tddy-rpc-web/test-fixtures";
import type { WebviewIpcBridge } from "../transport.js";

export interface WebviewIpcDouble extends WebviewIpcBridge {
  /** The next request frame the page sends, awaited rather than polled. */
  nextRequest(): Promise<RpcRequest>;
  /** The epoch the page registered its response channel with. */
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

export function aWebviewIpcDouble(): WebviewIpcDouble {
  const arrived: RpcRequest[] = [];
  const waiting: Array<(request: RpcRequest) => void> = [];
  let onFrame: ((frame: Uint8Array) => void) | null = null;
  let epoch = 0;
  let reportClosed: (reason: string) => void = () => {};
  const closed = new Promise<string>((resolve) => {
    reportClosed = resolve;
  });

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
    async connect(handler, clientEpoch) {
      onFrame = handler;
      epoch = clientEpoch;
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
    nextRequest() {
      const ready = arrived.shift();
      if (ready) {
        return Promise.resolve(ready);
      }
      return new Promise<RpcRequest>((resolve) => waiting.push(resolve));
    },
    connectedEpoch() {
      return epoch;
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

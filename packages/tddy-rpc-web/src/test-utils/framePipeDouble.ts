/**
 * An in-memory {@link FramePipe} standing in for a real byte pipe: it records the request frames a
 * transport sends and lets a test push response frames back, so envelope behaviour is observable
 * without a LiveKit room or a webview.
 */

import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import {
  RpcRequestSchema,
  RpcResponseSchema,
  type RpcRequest,
} from "../gen/rpc_envelope_pb.js";
import {
  EchoResponseSchema,
  type EchoResponse,
} from "../gen/test/echo_service_pb.js";
import type { FrameListener, FramePipe } from "../envelope-transport.js";

export interface FramePipeDouble extends FramePipe {
  /** Every request frame the transport has sent, in order, decoded. */
  sentRequests(): RpcRequest[];
  /** Push one response frame to the transport. */
  respond(response: MessageInitShape<typeof RpcResponseSchema>): void;
  /** Report the pipe permanently gone. */
  closeWith(reason: string): void;
}

export function aFramePipe(): FramePipeDouble {
  const sent: RpcRequest[] = [];
  const listeners: FrameListener[] = [];

  return {
    send(frame: Uint8Array) {
      sent.push(fromBinary(RpcRequestSchema, frame));
    },
    subscribe(listener: FrameListener) {
      listeners.push(listener);
      return () => {
        const at = listeners.indexOf(listener);
        if (at >= 0) {
          listeners.splice(at, 1);
        }
      };
    },
    sentRequests() {
      return sent;
    },
    respond(response) {
      const frame = toBinary(RpcResponseSchema, create(RpcResponseSchema, response));
      for (const listener of listeners) {
        listener.onFrame(frame);
      }
    },
    closeWith(reason) {
      for (const listener of listeners) {
        listener.onClose(reason);
      }
    },
  };
}

/** An `EchoResponse` body, encoded the way the host would put it on the wire. */
export function anEchoResponseBody(message: string): Uint8Array {
  return toBinary(EchoResponseSchema, create(EchoResponseSchema, { message }));
}

/** Decode an `EchoResponse` body from a frame the host sent. */
export function anEchoResponseFrom(body: Uint8Array): EchoResponse {
  return fromBinary(EchoResponseSchema, body);
}

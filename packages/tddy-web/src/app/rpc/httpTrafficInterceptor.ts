/**
 * createTrafficInterceptor — ConnectRPC Interceptor that measures outbound
 * (request) and inbound (response) bytes for unary calls by serializing the
 * protobuf messages to binary and recording the byte lengths on a meter.
 *
 * Streaming calls are passed through unchanged.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { create, toBinary } from "@bufbuild/protobuf";
import type { Interceptor } from "@connectrpc/connect";

export interface ByteMeter {
  record(dir: "in" | "out", bytes: number): void;
}

/**
 * Returns a ConnectRPC interceptor that measures binary-serialized byte
 * lengths of unary request and response messages.
 */
export function createTrafficInterceptor(meter: ByteMeter): Interceptor {
  return (next) => async (req) => {
    if (req.stream) {
      // Pass streaming calls through without measurement
      return next(req);
    }

    // Measure outbound bytes before calling next.
    // create() normalises a plain object init shape into a proper protobuf message.
    try {
      const msg = create(req.method.input as any, req.message as any);
      const outBytes = toBinary(req.method.input as any, msg);
      meter.record("out", outBytes.length);
    } catch {
      // Ignore serialization errors — don't break the call
    }

    const res = await next(req);

    // Measure inbound bytes from the response
    try {
      const msg = create(req.method.output as any, (res as any).message as any);
      const inBytes = toBinary(req.method.output as any, msg);
      meter.record("in", inBytes.length);
    } catch {
      // Ignore serialization errors — don't break the call
    }

    return res;
  };
}

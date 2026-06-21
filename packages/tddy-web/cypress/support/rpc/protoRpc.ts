/**
 * Low-level wire plumbing for proto-over-HTTP intercepts.
 *
 * Single home for the `toArrayBuffer` + intercept boilerplate that was previously
 * copy-pasted across ConnectionScreen.cy.tsx, App.cy.tsx, AppRouting.cy.tsx, and
 * RpcPlaygroundScreen.cy.tsx.
 */

import { type DescMessage, type MessageShape, toBinary } from "@bufbuild/protobuf";

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

/** Convert a Uint8Array to an ArrayBuffer for cy.intercept reply bodies. */
export function toArrayBuffer(u8: Uint8Array): ArrayBuffer {
  const buf = new ArrayBuffer(u8.length);
  new Uint8Array(buf).set(u8);
  return buf;
}

/**
 * Decode a raw cy.intercept request body (ArrayBuffer | Buffer | Uint8Array | string)
 * into a Uint8Array so callers can pass it to `fromBinary(schema, bytes)`.
 *
 * Replaces the inline `connectRequestBodyToUint8` function in ConnectionScreen.cy.tsx.
 */
export function decodeProtoRequestBody(body: unknown): Uint8Array {
  if (body instanceof Uint8Array) return body;
  if (body instanceof ArrayBuffer) return new Uint8Array(body);
  if (typeof Buffer !== "undefined" && Buffer.isBuffer(body)) {
    return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
  }
  if (typeof body === "string") {
    const out = new Uint8Array(body.length);
    for (let i = 0; i < body.length; i++) {
      out[i] = body.charCodeAt(i) & 0xff;
    }
    return out;
  }
  throw new Error(`Unsupported request body: ${Object.prototype.toString.call(body)}`);
}

// ---------------------------------------------------------------------------
// Generic intercept helper
// ---------------------------------------------------------------------------

/**
 * Register a cy.intercept for a Connect-RPC POST endpoint that replies with a
 * proto-encoded response.
 *
 * URL pattern is standardised to `**/rpc/<serviceMethod>`.
 *
 * @param serviceMethod  e.g. `"auth.AuthService/GetAuthStatus"`
 * @param schema         The protobuf descriptor schema for the response.
 * @param message        The message to encode as the response body.
 * @param alias          Optional cy alias (defaults to a camelCase derivation of serviceMethod).
 */
export function interceptProtoRpc<T extends DescMessage>(
  serviceMethod: string,
  schema: T,
  message: MessageShape<T>,
  alias?: string,
): void {
  const body = toArrayBuffer(toBinary(schema, message));
  const derived = alias ?? deriveAlias(serviceMethod);
  cy.intercept("POST", `**/rpc/${serviceMethod}`, (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body,
    });
  }).as(derived);
}

/** Derive a cy alias from a service method string, e.g. "auth.AuthService/GetAuthStatus" → "getAuthStatus". */
function deriveAlias(serviceMethod: string): string {
  const method = serviceMethod.split("/").pop() ?? serviceMethod;
  return method.charAt(0).toLowerCase() + method.slice(1);
}

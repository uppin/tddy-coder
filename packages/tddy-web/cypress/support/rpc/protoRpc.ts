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
  return decodeProtoRequestBodyRaw(body);
}

/**
 * Like decodeProtoRequestBody but strips the 5-byte ConnectRPC envelope header first.
 *
 * Server-streaming RPC requests use `application/connect+proto` which wraps each message
 * with a 5-byte header: 1 flag byte + 4-byte big-endian length. Unary requests use raw
 * proto without any framing. Use this variant when intercepting a server-streaming endpoint.
 */
export function decodeConnectStreamRequestBody(body: unknown): Uint8Array {
  const raw = decodeProtoRequestBodyRaw(body);
  // Sanity check: first byte is flags (0x00 = no compression), next 4 are length.
  if (raw.length >= 5 && (raw[0] === 0x00 || raw[0] === 0x01)) {
    return raw.slice(5);
  }
  return raw;
}

function decodeProtoRequestBodyRaw(body: unknown): Uint8Array {
  if (body instanceof Uint8Array) return body;
  // Use toString check for cross-realm ArrayBuffer (Cypress intercept handlers may run in a
  // different JS realm where `instanceof ArrayBuffer` returns false).
  if (body instanceof ArrayBuffer || Object.prototype.toString.call(body) === "[object ArrayBuffer]") {
    return new Uint8Array(body as ArrayBuffer);
  }
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
 * URL pattern matches any path ending in /rpc/<serviceMethod>.
 *
 * @param serviceMethod  e.g. "auth.AuthService/GetAuthStatus"
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

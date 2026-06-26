/**
 * Cypress RPC intercept helpers for VncService.
 *
 * Mirrors the pattern in connectionRpcs.ts: each helper registers a cy.intercept
 * for one VncService RPC method and replies with a canned protobuf response.
 *
 * NOTE: The generated vnc_pb.ts does not exist yet (requires buf generate after proto
 * is added to tddy-service). Until then, helpers serialize responses manually using
 * empty protobuf binary blobs (all-zero fields = default values).
 */

import { toArrayBuffer } from "./protoRpc";

// ---------------------------------------------------------------------------
// Raw empty-response helper
// ---------------------------------------------------------------------------

/** The minimal valid protobuf binary for a message with all default fields. */
const EMPTY_PROTO_BYTES = new Uint8Array(0);

function emptyProtoBody(): ArrayBuffer {
  return toArrayBuffer(EMPTY_PROTO_BYTES);
}

// ---------------------------------------------------------------------------
// Intercept helpers
// ---------------------------------------------------------------------------

/** Intercept ListVncTargets and reply with an empty target list. */
export function interceptListVncTargets(
  targets: Array<{ id: string; label: string; host: string; port: number }> = [],
): void {
  // Build a minimal protobuf ListVncTargetsResponse.
  // Field 1 (repeated VncTarget): encode each target as a length-delimited message.
  // For stub purposes we use an empty response (no targets) by default.
  void targets; // FIXME: encode real targets when vnc_pb.ts is generated

  cy.intercept("POST", "**/rpc/vnc.VncService/ListVncTargets", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: emptyProtoBody(),
    });
  }).as("listVncTargets");
}

/** Intercept AddVncTarget and reply with a canned VncTarget. */
export function interceptAddVncTarget(
  result: { id: string; label: string; host: string; port: number } = {
    id: "vnc-target-001",
    label: "Test VM",
    host: "192.168.1.1",
    port: 5900,
  },
): void {
  void result; // FIXME: encode target in response when vnc_pb.ts is generated
  cy.intercept("POST", "**/rpc/vnc.VncService/AddVncTarget", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: emptyProtoBody(),
    });
  }).as("addVncTarget");
}

/** Intercept RemoveVncTarget and reply with ok=true. */
export function interceptRemoveVncTarget(): void {
  cy.intercept("POST", "**/rpc/vnc.VncService/RemoveVncTarget", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: emptyProtoBody(),
    });
  }).as("removeVncTarget");
}

/** Intercept UnlockVncVault and reply with ok=true. */
export function interceptUnlockVncVault(succeed = true): void {
  if (succeed) {
    cy.intercept("POST", "**/rpc/vnc.VncService/UnlockVncVault", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: emptyProtoBody(),
      });
    }).as("unlockVncVault");
  } else {
    cy.intercept("POST", "**/rpc/vnc.VncService/UnlockVncVault", (req) => {
      req.reply({
        statusCode: 412,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code: "unauthenticated", message: "wrong passphrase" }),
      });
    }).as("unlockVncVault");
  }
}

/** Intercept StartVncStream and reply with LiveKit stream coordinates. */
export function interceptStartVncStream(
  overrides: Partial<{
    livekitRoom: string;
    livekitUrl: string;
    bridgeIdentity: string;
    trackName: string;
    width: number;
    height: number;
  }> = {},
): void {
  void overrides; // FIXME: encode in response when vnc_pb.ts is generated
  cy.intercept("POST", "**/rpc/vnc.VncService/StartVncStream", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: emptyProtoBody(),
    });
  }).as("startVncStream");
}

/** Intercept StopVncStream and reply with ok=true. */
export function interceptStopVncStream(): void {
  cy.intercept("POST", "**/rpc/vnc.VncService/StopVncStream", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: emptyProtoBody(),
    });
  }).as("stopVncStream");
}

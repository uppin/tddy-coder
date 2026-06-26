/**
 * Cypress RPC intercept helpers for VncService.
 *
 * Mirrors the pattern in connectionRpcs.ts: each helper registers a cy.intercept
 * for one VncService RPC method and replies with a canned protobuf response.
 */

import { create, toBinary } from "@bufbuild/protobuf";
import {
  AddVncTargetResponseSchema,
  ListVncTargetsResponseSchema,
  RemoveVncTargetResponseSchema,
  StartVncStreamResponseSchema,
  StopVncStreamResponseSchema,
  UnlockVncVaultResponseSchema,
  VncTargetSchema,
} from "../../../src/gen/vnc_pb";
import { toArrayBuffer } from "./protoRpc";

// ---------------------------------------------------------------------------
// Intercept helpers
// ---------------------------------------------------------------------------

/** Intercept ListVncTargets and reply with the given target list (default empty). */
export function interceptListVncTargets(
  targets: Array<{ id: string; label: string; host: string; port: number }> = [],
): void {
  const body = toArrayBuffer(
    toBinary(
      ListVncTargetsResponseSchema,
      create(ListVncTargetsResponseSchema, {
        targets: targets.map((t) =>
          create(VncTargetSchema, { id: t.id, label: t.label, host: t.host, port: t.port }),
        ),
      }),
    ),
  );
  cy.intercept("POST", "**/rpc/vnc.VncService/ListVncTargets", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
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
  const body = toArrayBuffer(
    toBinary(
      AddVncTargetResponseSchema,
      create(AddVncTargetResponseSchema, {
        target: create(VncTargetSchema, {
          id: result.id,
          label: result.label,
          host: result.host,
          port: result.port,
        }),
      }),
    ),
  );
  cy.intercept("POST", "**/rpc/vnc.VncService/AddVncTarget", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("addVncTarget");
}

/** Intercept RemoveVncTarget and reply with ok=true. */
export function interceptRemoveVncTarget(): void {
  const body = toArrayBuffer(
    toBinary(RemoveVncTargetResponseSchema, create(RemoveVncTargetResponseSchema, { ok: true })),
  );
  cy.intercept("POST", "**/rpc/vnc.VncService/RemoveVncTarget", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("removeVncTarget");
}

/** Intercept UnlockVncVault and reply with ok=true or an auth error. */
export function interceptUnlockVncVault(succeed = true): void {
  if (succeed) {
    const body = toArrayBuffer(
      toBinary(
        UnlockVncVaultResponseSchema,
        create(UnlockVncVaultResponseSchema, { ok: true }),
      ),
    );
    cy.intercept("POST", "**/rpc/vnc.VncService/UnlockVncVault", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
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
  const body = toArrayBuffer(
    toBinary(
      StartVncStreamResponseSchema,
      create(StartVncStreamResponseSchema, {
        livekitRoom: overrides.livekitRoom ?? "room-vnc-test",
        livekitUrl: overrides.livekitUrl ?? "ws://127.0.0.1:7880",
        bridgeIdentity: overrides.bridgeIdentity ?? "vnc-test-session-t-001",
        trackName: overrides.trackName ?? "vnc:t-001",
        width: overrides.width ?? 1920,
        height: overrides.height ?? 1080,
      }),
    ),
  );
  cy.intercept("POST", "**/rpc/vnc.VncService/StartVncStream", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("startVncStream");
}

/** Intercept StopVncStream and reply with ok=true. */
export function interceptStopVncStream(): void {
  const body = toArrayBuffer(
    toBinary(StopVncStreamResponseSchema, create(StopVncStreamResponseSchema, { ok: true })),
  );
  cy.intercept("POST", "**/rpc/vnc.VncService/StopVncStream", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
  }).as("stopVncStream");
}

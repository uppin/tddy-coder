/**
 * Bug reproduction: GrpcSessionTerminal.send() has two interrelated defects.
 *
 * Bug A — Unhandled promise rejection:
 *   send() calls `void client.sendTerminalInput(...)`, discarding the promise.
 *   When the RPC returns [failed_precondition] "terminal controlled by another screen",
 *   the rejection surfaces as an uncaught exception in the browser console.
 *
 * Bug B — Control token never forwarded:
 *   useTerminalControl (inside SessionMainPane) successfully calls ClaimTerminalControl
 *   and receives a token, but GrpcSessionTerminal.send() never passes it to
 *   sendTerminalInput. The daemon rejects every SendTerminalInput with 400
 *   [failed_precondition] even when this screen IS the controller — because the token
 *   field is absent from the request.
 *
 * Expected after fix:
 *   - send() must catch errors from sendTerminalInput (no unhandled rejections).
 *   - GrpcSessionTerminal must accept a `controlToken` prop (or ref) and include it
 *     in every sendTerminalInput call.
 *
 * Steps that reproduce in the browser:
 *   1. Open a session tab (#/sessions/<id>)
 *   2. The terminal mounts, fit() fires → onResize → stream.send → sendTerminalInput
 *   3. Console: POST .../SendTerminalInput 400 (Bad Request)
 *   4. Console: Uncaught (in promise) ConnectError: [failed_precondition] terminal controlled by another screen
 */

import React, { useMemo } from "react";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  ClaimTerminalControlResponseSchema,
  SessionTerminalInputSchema,
  SendTerminalInputResponseSchema,
} from "../../src/gen/connection_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { decodeProtoRequestBody, interceptProtoRpc, toArrayBuffer } from "../support/rpc/protoRpc";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "send-error-test-session-1a2b";
const SESSION_TOKEN = "test-session-token-xyz";
const CONTROL_TOKEN = "granted-control-token-abc";

const OK_SEND_INPUT = toArrayBuffer(
  toBinary(SendTerminalInputResponseSchema, create(SendTerminalInputResponseSchema, {})),
);

/** Empty StreamTerminalOutput stream — no output data, stream ends immediately. */
function interceptStreamTerminalOutput() {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/StreamTerminalOutput", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: new ArrayBuffer(0) });
  }).as("streamTerminalOutput");
}

/** ClaimTerminalControl → granted=true with a known token. */
function interceptClaimTerminalControl(token = CONTROL_TOKEN) {
  interceptProtoRpc(
    "connection.ConnectionService/ClaimTerminalControl",
    ClaimTerminalControlResponseSchema,
    create(ClaimTerminalControlResponseSchema, { granted: true, controlToken: token }),
    "claimTerminalControl",
  );
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

/**
 * Renders GrpcSessionTerminal with a real HTTP transport that Cypress can intercept.
 * Passes `controlToken` through once the prop exists (currently causes a TS error
 * that is suppressed with @ts-expect-error — this is intentional: the prop must be
 * added as part of the bug fix).
 */
function Harness({ controlToken }: { controlToken?: string }) {
  const transport = useMemo(
    () => createConnectTransport({ baseUrl: `${window.location.origin}/rpc`, useBinaryFormat: true }),
    [],
  );
  const client = useMemo(() => createClient(ConnectionService, transport), [transport]);

  return (
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <GrpcSessionTerminal
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        client={client}
        controlToken={controlToken}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// AC1: send() errors must not produce unhandled promise rejections
// ---------------------------------------------------------------------------

describe("GrpcSessionTerminal — SendTerminalInput error handling", () => {
  it("does not produce an unhandled promise rejection when SendTerminalInput returns [failed_precondition]", () => {
    // Given — output stream is open but silent
    interceptStreamTerminalOutput();

    // Given — every SendTerminalInput call fails with the control-mutex error
    cy.intercept("POST", "**/rpc/connection.ConnectionService/SendTerminalInput", (req) => {
      req.reply({
        statusCode: 400,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          code: "failed_precondition",
          message: "terminal controlled by another screen",
        }),
      });
    }).as("sendTerminalInput");

    // Collect uncaught exceptions (suppress Cypress auto-fail so we can assert explicitly)
    const unhandledRejections: string[] = [];
    cy.on("uncaught:exception", (err) => {
      unhandledRejections.push(err.message);
      return false;
    });

    // When — mount; terminal will fit() → onResize → stream.send → sendTerminalInput
    cy.mount(<Harness />);

    // When — wait for the resize to reach the server so we know the RPC was triggered
    cy.wait("@sendTerminalInput");

    // Then — the error must have been caught inside send() and must not propagate
    cy.wrap(null).should(() => {
      const control = unhandledRejections.filter(
        (m) => m.includes("failed_precondition") || m.includes("terminal controlled"),
      );
      expect(control).to.have.length(
        0,
        "SendTerminalInput errors must be caught inside send() — not thrown as uncaught promise rejections. " +
          `Unhandled: [${unhandledRejections.join("; ")}]`,
      );
    });
  });

  // ---------------------------------------------------------------------------
  // AC2: controlToken from ClaimTerminalControl must be forwarded in every
  //      sendTerminalInput request. Without this the daemon rejects even when
  //      this screen successfully claimed control.
  // ---------------------------------------------------------------------------

  it("includes the control token in every SendTerminalInput request", () => {
    // Given — output stream open
    interceptStreamTerminalOutput();

    // Given — sendTerminalInput succeeds; capture every request body
    const capturedRequests: ReturnType<typeof fromBinary<typeof SessionTerminalInputSchema>>[] = [];
    cy.intercept("POST", "**/rpc/connection.ConnectionService/SendTerminalInput", (req) => {
      const decoded = fromBinary(SessionTerminalInputSchema, decodeProtoRequestBody(req.body));
      capturedRequests.push(decoded);
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: OK_SEND_INPUT });
    }).as("sendTerminalInput");

    // When — mount with a known control token (the prop to be added in the fix)
    cy.mount(<Harness controlToken={CONTROL_TOKEN} />);

    // When — wait for at least one send (the resize on mount)
    cy.wait("@sendTerminalInput");

    // Then — every captured request must carry the control token
    cy.wrap(null).should(() => {
      expect(capturedRequests.length).to.be.greaterThan(0, "at least one sendTerminalInput should have been sent");
      for (const req of capturedRequests) {
        expect(req.controlToken).to.equal(
          CONTROL_TOKEN,
          `sendTerminalInput must forward the controlToken — got '${req.controlToken}' instead of '${CONTROL_TOKEN}'`,
        );
      }
    });
  });
});

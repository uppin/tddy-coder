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
 * On the bidi wire (`StreamSessionTerminalIO`) input no longer travels as one unary call per send:
 * every keystroke/resize is a later message on the stream whose opening message asked for the
 * snapshot, each presenting the control token held at send time. A daemon refusal of input is the
 * stream failing with [failed_precondition]. The two defects above map onto that wire as:
 *   - a refused stream must be caught by the terminal (no unhandled rejection);
 *   - every input message must carry the claimed control token.
 * connect-web cannot carry a bidi call over HTTP, so the daemon is an in-memory
 * `TerminalSessionService` whose stream records every message it is sent.
 *
 * Steps that reproduced in the browser (split unary wire):
 *   1. Open a session tab (#/sessions/<id>)
 *   2. The terminal mounts, fit() fires → onResize → stream.send → sendTerminalInput
 *   3. Console: POST .../SendTerminalInput 400 (Bad Request)
 *   4. Console: Uncaught (in promise) ConnectError: [failed_precondition] terminal controlled by another screen
 */

import React from "react";
import { Code, ConnectError, createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { TerminalSessionService, type SessionTerminalInput } from "../../src/gen/terminal_session_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import type { ConnectedSession } from "../../src/components/sessions/useTerminalControl";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "send-error-test-session-1a2b";
const SESSION_TOKEN = "test-session-token-xyz";
const CONTROL_TOKEN = "granted-control-token-abc";
const CONTROL_REFUSAL = "terminal controlled by another screen";

type TerminalClient = Client<typeof TerminalSessionService>;

/**
 * A daemon serving the bidi terminal stream in memory. Its output side is silent; every input
 * message after the opening one is recorded. With `refuseInput`, the first input message is
 * answered by failing the whole stream with the control-mutex error — the bidi form of every
 * SendTerminalInput returning [failed_precondition].
 */
function aTerminalDaemon({ refuseInput = false }: { refuseInput?: boolean } = {}) {
  const inputMessages: SessionTerminalInput[] = [];
  const backend = anInMemoryRpcBackend().implement(TerminalSessionService, {
    async *streamSessionTerminalIO(requests: AsyncIterable<SessionTerminalInput>) {
      const reader = requests[Symbol.asyncIterator]();
      const opening = await reader.next();
      if (opening.done) return;
      if (refuseInput) {
        const input = await reader.next();
        if (input.done) return;
        inputMessages.push(input.value);
        throw new ConnectError(CONTROL_REFUSAL, Code.FailedPrecondition);
      }
      void (async () => {
        for (let next = await reader.next(); !next.done; next = await reader.next()) {
          inputMessages.push(next.value);
        }
      })();
      await new Promise<never>(() => undefined);
    },
  });
  const client: TerminalClient = createClient(TerminalSessionService, backend.transport());
  return { client, inputMessages };
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

/**
 * Renders GrpcSessionTerminal against the in-memory daemon. `connected` is the claimed lease — when
 * non-null, input goes up the stream carrying the token; when `null`, input is queued (the onReady
 * resize is held until the claim resolves) and no input message is sent.
 */
function Harness({ client, connected }: { client: TerminalClient; connected: ConnectedSession | null }) {
  return (
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <UploadProgressProvider>
        <GrpcSessionTerminal
          sessionId={SESSION_ID}
          sessionToken={SESSION_TOKEN}
          client={client}
          connected={connected}
        />
      </UploadProgressProvider>
    </div>
  );
}

/** Waits until the daemon has received at least one input message (the resize on mount). */
function waitForInput(daemon: { inputMessages: SessionTerminalInput[] }) {
  cy.wrap(daemon, { timeout: 10000 }).should((d) => {
    expect(d.inputMessages.length, "at least one input message reached the daemon").to.be.greaterThan(0);
  });
}

// ---------------------------------------------------------------------------
// AC1: input refusals must not produce unhandled promise rejections
// ---------------------------------------------------------------------------

describe("GrpcSessionTerminal — terminal input error handling", () => {
  it("does not produce an unhandled promise rejection when the daemon refuses input with [failed_precondition]", () => {
    // Given — output stream is open but silent, and the daemon refuses input with the
    // control-mutex error (the bidi stream fails with [failed_precondition])
    const daemon = aTerminalDaemon({ refuseInput: true });

    // Collect uncaught exceptions (suppress Cypress auto-fail so we can assert explicitly)
    const unhandledRejections: string[] = [];
    cy.on("uncaught:exception", (err) => {
      unhandledRejections.push(err.message);
      return false;
    });

    // When — mount with an active lease so the resize reaches the daemon (input is only sent
    // once `connected` is non-null; with `null` it would be queued and never fire). The daemon
    // refuses input regardless of token.
    cy.mount(
      <Harness client={daemon.client} connected={{ sessionId: SESSION_ID, controlToken: "any-lease-token" }} />,
    );

    // When — wait for the resize to reach the daemon so we know input was sent and refused
    waitForInput(daemon);

    // Then — the refusal reached the terminal and was handled there
    byTestId(TEST_IDS.terminalInputError, { timeout: 10000 }).should("contain.text", CONTROL_REFUSAL);

    // Then — the error must have been caught and must not propagate
    cy.wrap(null).should(() => {
      const control = unhandledRejections.filter(
        (m) => m.includes("failed_precondition") || m.includes("terminal controlled"),
      );
      expect(control).to.have.length(
        0,
        "terminal input errors must be caught by the terminal — not thrown as uncaught promise rejections. " +
          `Unhandled: [${unhandledRejections.join("; ")}]`,
      );
    });
  });

  // ---------------------------------------------------------------------------
  // AC2: controlToken from ClaimTerminalControl must be forwarded on every input
  //      message. Without this the daemon rejects even when this screen
  //      successfully claimed control.
  // ---------------------------------------------------------------------------

  it("includes the control token in every terminal input message", () => {
    // Given — output stream open; every input message is captured
    const daemon = aTerminalDaemon();

    // When — mount with a claimed lease carrying the known control token
    cy.mount(<Harness client={daemon.client} connected={{ sessionId: SESSION_ID, controlToken: CONTROL_TOKEN }} />);

    // When — wait for at least one send (the resize on mount)
    waitForInput(daemon);

    // Then — every captured input message must carry the control token
    cy.wrap(null).should(() => {
      expect(daemon.inputMessages.length).to.be.greaterThan(0, "at least one input message should have been sent");
      for (const req of daemon.inputMessages) {
        expect(req.controlToken).to.equal(
          CONTROL_TOKEN,
          `terminal input must forward the controlToken — got '${req.controlToken}' instead of '${CONTROL_TOKEN}'`,
        );
      }
    });
  });
});

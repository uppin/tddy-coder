/**
 * Regression test: the connected lease (`ConnectedSession`) is dropped when the focused session
 * changes, so a stale token from a previous session never leaks into the newly focused session's
 * terminal input (the "terminal controlled by another screen" failures on fast session change).
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`.
 *
 * Each session runtime owns its own `useTerminalControl` hook (and therefore its own `connected`
 * state). This test exercises the hook's drop-on-session-change invariant directly via a harness:
 * when the session id changes A → B, `connected` flips to `null` before B's claim resolves, so B's
 * terminal cannot send input (there is no `ConnectedSession` to send with) until B's own lease is
 * granted.
 */

import React, { useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ClaimTerminalControlResponseSchema,
  ConnectionService,
} from "../../src/gen/connection_pb";
import { useTerminalControl, type Session } from "../../src/components/sessions/useTerminalControl";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_A = "session-aaaaaaaa-0000-0000-0000-000000000001";
const SESSION_B = "session-bbbbbbbb-0000-0000-0000-000000000002";

/**
 * In-memory `ConnectionService` backend that grants a DISTINCT control token per session. Session
 * B's claim is held pending until the test resolves it, so the drop-to-null window (between the
 * switch and B's grant) is observable. A real RPC is slow enough that this window is naturally
 * visible in production.
 */
function aControlBackendGrantingDistinctTokens() {
  let resolveBClaim: () => void = () => undefined;
  const bClaimPending = new Promise<void>((resolve) => {
    resolveBClaim = resolve;
  });

  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    claimTerminalControl: async (req: { sessionId: string }) => {
      if (req.sessionId === SESSION_B) {
        await bClaimPending;
      }
      return create(ClaimTerminalControlResponseSchema, {
        granted: true,
        controlToken: `token-${req.sessionId}`,
      });
    },
    // Server-streaming control watch — yield nothing; the token comes from the claim response and
    // the hook only needs the stream to not error.
    watchTerminalControl: async function* () {
      yield {
        $typeName: "connection.TerminalControlEvent",
        event: { case: "granted", value: "" },
      } as any;
    },
  });

  return { backend, resolveBClaim: () => resolveBClaim() };
}

// ---------------------------------------------------------------------------
// Harness + fluent driver
// ---------------------------------------------------------------------------

interface ConnectedLeaseHarnessProps {
  client: ReturnType<typeof createClient<typeof ConnectionService>>;
  initialSessionId: string;
  nextSessionId: string;
  onConnectedValue: (value: string) => void;
}

/** Renders the connected lease's token (empty when `connected === null`) and reports every rendered
 *  value via `onConnectedValue`, so the driver can assert the lease history across a session switch. */
function ConnectedLeaseHarness({
  client,
  initialSessionId,
  nextSessionId,
  onConnectedValue,
}: ConnectedLeaseHarnessProps) {
  const [sessionId, setSessionId] = useState(initialSessionId);
  // A `Session` is the daemon reference (sessionId + client); the hook converts it into a
  // `ConnectedSession` once the claim resolves. `connected` is `null` until then.
  const session: Session = { sessionId, client };
  const { connected } = useTerminalControl(session, "fake-session-token");
  const value = connected?.controlToken ?? "";
  onConnectedValue(value);
  return (
    <>
      <span data-testid={TEST_IDS.controlTokenDisplay}>{value}</span>
      <button data-testid={TEST_IDS.switchSession} onClick={() => setSessionId(nextSessionId)}>
        switch session
      </button>
    </>
  );
}

/**
 * Fluent driver for the connected-lease reset harness. Encapsulates mounting, the lease display,
 * the session-switch action, the deferred-claim trigger, and the rendered-value history so the test
 * body stays free of selectors and framework wiring.
 */
function aConnectedLeaseHarness() {
  const { backend, resolveBClaim } = aControlBackendGrantingDistinctTokens();
  const client = createClient(ConnectionService, backend.transport());
  const onConnectedValue = cy.stub().as("onConnectedValue");
  const leaseDisplay = () => byTestId(TEST_IDS.controlTokenDisplay);
  const switchButton = () => byTestId(TEST_IDS.switchSession);

  return {
    mountAttachedTo(initialSessionId: string, nextSessionId: string) {
      cy.mount(
        <ConnectedLeaseHarness
          client={client}
          initialSessionId={initialSessionId}
          nextSessionId={nextSessionId}
          onConnectedValue={onConnectedValue}
        />,
      );
      return this;
    },
    /** Wait for the rendered lease display to equal `value` ("" means `connected === null`). */
    expectLeaseDisplay(value: string) {
      leaseDisplay().should("have.text", value);
      return this;
    },
    /** Switch the harness to the next session (simulates focusing a different session). */
    switchSession() {
      switchButton().click();
      return this;
    },
    /** Clear the recorded value history so subsequent `expectLeaseValueWasRendered` checks cover
     *  only the window after this point. */
    forgetLeaseHistorySoFar() {
      cy.get("@onConnectedValue").invoke("resetHistory");
      return this;
    },
    /** Assert the harness rendered the given lease value at least once since the last reset. */
    expectLeaseValueWasRendered(value: string) {
      cy.get("@onConnectedValue").should("have.been.calledWith", value);
      return this;
    },
    /** Resolve session B's pending claim so its lease is granted. Deferred into the command chain so
     *  it runs after the preceding assertions (a bare synchronous call would execute before any `cy`
     *  command enqueued by the test). */
    resolvePendingClaim() {
      cy.then(() => resolveBClaim());
      return this;
    },
  };
}

// ---------------------------------------------------------------------------

it("drops the connected lease (connected → null) when the focused session changes, so the next session never sees the previous session's token", () => {
  // Given — a harness attached to session A, with a backend that grants `token-<sessionId>` per
  // session and holds session B's claim pending until the test resolves it.
  const harness = aConnectedLeaseHarness();
  harness
    .mountAttachedTo(SESSION_A, SESSION_B)
    .expectLeaseDisplay(`token-${SESSION_A}`)
    .forgetLeaseHistorySoFar();

  // When — the user switches focus to session B
  harness.switchSession();

  // Then — `connected` flips to null in the switch window (B's claim is still pending), so the
  // display is empty. This is the drop that prevents B's terminal input from carrying A's stale
  // lease token: without it, the display would stay "token-A" until B's claim resolved. (In
  // production, `SessionRuntime` is keyed by sessionId, so switching mounts a fresh runtime with a
  // null `connected`; this harness exercises the hook's drop-on-change invariant for the
  // same-instance path.)
  harness.expectLeaseDisplay("").expectLeaseValueWasRendered("");

  // And — once B's claim resolves, B's own lease is granted (never A's).
  harness.resolvePendingClaim().expectLeaseDisplay(`token-${SESSION_B}`);
});

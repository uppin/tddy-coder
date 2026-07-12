/**
 * Regression test: the terminal control token is reset when the focused session changes, so a
 * stale token from a previous session never leaks into the newly focused session's terminal input
 * (the "terminal controlled by another screen" failures on fast session change).
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`.
 *
 * Each session runtime owns its own `useTerminalControl` hook (and therefore its own
 * `controlTokenRef`). This test exercises the hook's reset-on-session-change invariant directly via
 * a harness: when the session id changes A → B, the token is cleared to "" before B's claim resolves,
 * so B's terminal never reads A's stale token.
 */

import React, { useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ClaimTerminalControlResponseSchema,
  ConnectionService,
} from "../../src/gen/connection_pb";
import { useTerminalControl } from "../../src/components/sessions/useTerminalControl";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_A = "session-aaaaaaaa-0000-0000-0000-000000000001";
const SESSION_B = "session-bbbbbbbb-0000-0000-0000-000000000002";

/**
 * In-memory `ConnectionService` backend that grants a DISTINCT control token per session. Session
 * B's claim is held pending until the test resolves it, so the reset-to-empty window (between the
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

interface ControlTokenHarnessProps {
  client: ReturnType<typeof createClient<typeof ConnectionService>>;
  initialSessionId: string;
  nextSessionId: string;
  onToken: (token: string) => void;
}

/** Renders the live control token and reports every rendered value via `onToken`, so the driver can
 *  assert the token history across a session switch. */
function ControlTokenHarness({ client, initialSessionId, nextSessionId, onToken }: ControlTokenHarnessProps) {
  const [sessionId, setSessionId] = useState(initialSessionId);
  const { controlTokenRef } = useTerminalControl(sessionId, "fake-session-token", client);
  const token = controlTokenRef.current;
  onToken(token);
  return (
    <>
      <span data-testid={TEST_IDS.controlTokenDisplay}>{token}</span>
      <button data-testid={TEST_IDS.switchSession} onClick={() => setSessionId(nextSessionId)}>
        switch session
      </button>
    </>
  );
}

/**
 * Fluent driver for the control-token reset harness. Encapsulates mounting, the token display, the
 * session-switch action, the deferred-claim trigger, and the rendered-token history so the test body
 * stays free of selectors and framework wiring.
 */
function aControlTokenHarness() {
  const { backend, resolveBClaim } = aControlBackendGrantingDistinctTokens();
  const client = createClient(ConnectionService, backend.transport());
  const onToken = cy.stub().as("onToken");
  const tokenDisplay = () => byTestId(TEST_IDS.controlTokenDisplay);
  const switchButton = () => byTestId(TEST_IDS.switchSession);

  return {
    mountAttachedTo(initialSessionId: string, nextSessionId: string) {
      cy.mount(
        <ControlTokenHarness
          client={client}
          initialSessionId={initialSessionId}
          nextSessionId={nextSessionId}
          onToken={onToken}
        />,
      );
      return this;
    },
    /** Wait for the rendered token display to equal `token`. */
    expectTokenDisplay(token: string) {
      tokenDisplay().should("have.text", token);
      return this;
    },
    /** Switch the harness to the next session (simulates focusing a different session). */
    switchSession() {
      switchButton().click();
      return this;
    },
    /** Clear the recorded token history so subsequent `expectTokenWasRendered` checks cover only the
     *  window after this point. */
    forgetTokenHistorySoFar() {
      cy.get("@onToken").invoke("resetHistory");
      return this;
    },
    /** Assert the harness rendered the given token at least once since the last history reset. */
    expectTokenWasRendered(token: string) {
      cy.get("@onToken").should("have.been.calledWith", token);
      return this;
    },
    /** Resolve session B's pending claim so its token is granted. Deferred into the command chain so
     *  it runs after the preceding assertions (a bare synchronous call would execute before any `cy`
     *  command enqueued by the test). */
    resolvePendingClaim() {
      cy.then(() => resolveBClaim());
      return this;
    },
  };
}

// ---------------------------------------------------------------------------

it("resets the control token to empty when the focused session changes, so the next session never sees the previous session's token", () => {
  // Given — a harness attached to session A, with a backend that grants `token-<sessionId>` per
  // session and holds session B's claim pending until the test resolves it.
  const harness = aControlTokenHarness();
  harness
    .mountAttachedTo(SESSION_A, SESSION_B)
    .expectTokenDisplay(`token-${SESSION_A}`)
    .forgetTokenHistorySoFar();

  // When — the user switches focus to session B
  harness.switchSession();

  // Then — the token is reset to "" in the switch window (B's claim is still pending). This is the
  // reset that prevents B's terminal input from carrying A's stale lease token: without it, the
  // display would stay "token-A" until B's claim resolved. (In production, `SessionRuntime` is
  // keyed by sessionId, so switching mounts a fresh runtime with an empty ref; this harness
  // exercises the hook's reset-on-change invariant for the same-instance path.)
  harness.expectTokenDisplay("").expectTokenWasRendered("");

  // And — once B's claim resolves, B's own token is granted (never A's).
  harness.resolvePendingClaim().expectTokenDisplay(`token-${SESSION_B}`);
});

/**
 * Structural regression guard for the input/claim race: input sent before the control lease is
 * granted (typically the terminal's onReady resize OSC) must be QUEUED, never sent with an empty
 * token, and flushed once `ConnectedSession` arrives — carrying the granted token.
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`.
 *
 * The race this prevents: before the type split, `sendTerminalInput` went out with an empty token
 * in the window between the terminal's mount-time resize and `ClaimTerminalControl` resolving, and
 * the daemon rejected it with "terminal controlled by another screen". With `Session` →
 * `ConnectedSession`, `sendTerminalInput` is only callable on a `ConnectedSession` (lease in hand):
 * the resize is queued while `connected === null` and flushed when the claim converts the session.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ClaimTerminalControlResponseSchema,
  ConnectionService,
  SendTerminalInputResponseSchema,
} from "../../src/gen/connection_pb";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { useTerminalControl, type Session } from "../../src/components/sessions/useTerminalControl";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "gating-session-aaaa-0000-0000-0000-000000000001";
const SESSION_TOKEN = "gating-session-token";
const CLAIM_TOKEN = "granted-lease-token-xyz";

type ConnectionClient = ReturnType<typeof createClient<typeof ConnectionService>>;

/**
 * In-memory `ConnectionService` backend whose `claimTerminalControl` is held pending until the test
 * resolves it, so the `connected === null` window (between attach and grant) is observable and
 * deterministic. `sendTerminalInput` records every call so the test can assert input never reaches
 * the daemon before the lease exists, and that the flushed input carries the token.
 */
function aDeferredClaimBackend() {
  let resolveClaim: () => void = () => undefined;
  const claimPending = new Promise<void>((resolve) => {
    resolveClaim = resolve;
  });
  const sendInputCalls: { sessionId: string; controlToken: string }[] = [];
  const state = { streamOpened: false };

  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    claimTerminalControl: async () => {
      await claimPending;
      return create(ClaimTerminalControlResponseSchema, { granted: true, controlToken: CLAIM_TOKEN });
    },
    // Server-streaming control watch — yield one event then end; the lease token comes from the
    // claim response and the hook only needs the stream to not error.
    watchTerminalControl: async function* () {
      yield {
        $typeName: "connection.TerminalControlEvent",
        event: { case: "granted", value: CLAIM_TOKEN },
      } as any;
    },
    // Server-streaming output — record that it opened, yield no data, and end. The terminal's
    // mount-time resize still fires a `send` independently of output data.
    streamTerminalOutput: async function* () {
      state.streamOpened = true;
    },
    sendTerminalInput: async (req: { sessionId: string; controlToken: string }) => {
      sendInputCalls.push({ sessionId: req.sessionId, controlToken: req.controlToken });
      return create(SendTerminalInputResponseSchema, {});
    },
  });

  return { backend, sendInputCalls, state, resolveClaim: () => resolveClaim() };
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/** Mounts `useTerminalControl` + `GrpcSessionTerminal` together: the hook converts the `Session`
 *  into a `ConnectedSession` once the (deferred) claim resolves, and the terminal gates input on
 *  that `connected` value — exactly the production wiring in `SessionRuntime`. */
function InputGatingHarness({ client }: { client: ConnectionClient }) {
  const session: Session = { sessionId: SESSION_ID, client };
  const { connected } = useTerminalControl(session, SESSION_TOKEN);
  return (
    <div style={{ width: 800, height: 400, position: "relative" }}>
      <GrpcSessionTerminal
        sessionId={SESSION_ID}
        sessionToken={SESSION_TOKEN}
        client={client}
        connected={connected}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

it("queues input sent before the claim resolves and flushes it once ConnectedSession arrives (no input races the claim)", () => {
  // Given — a backend whose claim is deferred until the test releases it, so `connected` stays
  // null after mount.
  const { backend, sendInputCalls, state, resolveClaim } = aDeferredClaimBackend();
  const client = createClient(ConnectionService, backend.transport());

  // When — the terminal mounts (auto-claim fires but is held pending)
  cy.mount(<InputGatingHarness client={client} />);

  // Wait for the output stream to open (mount completed) and give the terminal a frame to fire its
  // mount-time resize — that resize is the input that must be queued.
  cy.wrap(state).should((s) => expect(s.streamOpened, "streamTerminalOutput opened on mount").to.be.true);
  cy.wait(120);

  // Then — NO sendTerminalInput has reached the backend: the resize was queued because
  // `connected` is still null (the claim is pending). Structurally, input cannot go out before the
  // lease exists — this is the race that previously produced "terminal controlled by another
  // screen" rejections for the onReady resize.
  cy.wrap(sendInputCalls).should("have.length", 0);

  // When — the claim resolves and `connected` becomes a ConnectedSession carrying the granted token
  cy.then(() => resolveClaim());

  // Then — the queued input is flushed and reaches sendTerminalInput WITH the granted token (not
  // an empty string). Every flushed call must carry the lease.
  cy.wrap(sendInputCalls).should("have.length.greaterThan", 0);
  cy.wrap(sendInputCalls).should((calls) => {
    for (const c of calls) {
      expect(c.sessionId, "flushed input targets the mounted session").to.equal(SESSION_ID);
      expect(
        c.controlToken,
        "flushed input must carry the granted control token — not an empty/stale token",
      ).to.equal(CLAIM_TOKEN);
    }
  });
});

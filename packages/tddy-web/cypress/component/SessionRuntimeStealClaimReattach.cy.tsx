/**
 * Regression test: clicking "Claim terminal" ONCE must make this screen the controller and keep it
 * the controller. Previously a steal-claim updated local state but never re-subscribed the control
 * watch with the newly granted token, so the daemon's steal broadcast — re-validated against the
 * now-stale subscription token — reported `youAreController: false` and flipped the overlay back,
 * forcing the user to click "Claim terminal" a second time before the terminal opened.
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`,
 *          `docs/ft/daemon/terminal-sessions.md` (control lease).
 *
 * This exercises `useTerminalControl` directly against an in-memory `ConnectionService` that mirrors
 * the daemon control mutex (`CliSessionManager::claim_control`) and the token-revalidating watch
 * relay (`relay_control_events`): the watch recomputes `youAreController` on every control change by
 * re-validating the token the subscription was opened with, and a steal that evicts a previous
 * holder broadcasts a change to all open subscriptions.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ClaimTerminalControlResponseSchema,
  TerminalControlEventSchema,
  ConnectionService,
} from "../../src/gen/connection_pb";
import { useTerminalControl, type Session } from "../../src/components/sessions/useTerminalControl";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "session-cccccccc-0000-0000-0000-000000000003";
/** A different screen already holds the lease when this screen attaches, so the auto-claim is denied
 *  and the "Claim terminal" CTA would be shown. Distinct from this screen's own `getScreenId()`. */
const OTHER_SCREEN = "screen-held-by-another-0000";

interface Lease {
  token: string;
  holderScreenId: string;
}

/**
 * In-memory `ConnectionService` modelling the daemon terminal-control lease. A single lease per
 * session (`CliSessionManager::claim_control`): `steal:false` is denied when another screen holds
 * it; `steal:true` evicts the holder, mints a new token, and broadcasts a change to open watchers.
 * Each `watchTerminalControl` stream recomputes `youAreController` per change by re-validating the
 * token it was opened with — exactly as `relay_control_events` does via `verify_control`.
 */
function aStealClaimDaemon() {
  let lease: Lease = { token: "lease-token-held-by-other", holderScreenId: OTHER_SCREEN };
  let tokenSeq = 0;
  // Wake handles for open watch streams, resolved on each control change (the broadcast).
  let wakeResolvers: Array<() => void> = [];
  const broadcast = () => {
    const pending = wakeResolvers;
    wakeResolvers = [];
    for (const resolve of pending) resolve();
  };
  const nextBroadcast = () => new Promise<void>((resolve) => wakeResolvers.push(resolve));

  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    claimTerminalControl: async (req: { screenId: string; steal: boolean }) => {
      const heldByOther = lease.holderScreenId !== req.screenId;
      if (heldByOther && !req.steal) {
        return create(ClaimTerminalControlResponseSchema, {
          granted: false,
          currentHolderScreenId: lease.holderScreenId,
        });
      }
      const alreadyHolder = lease.holderScreenId === req.screenId;
      const token = alreadyHolder ? lease.token : `lease-token-${(tokenSeq += 1)}`;
      const evicts = heldByOther && req.steal;
      lease = { token, holderScreenId: req.screenId };
      // The daemon broadcasts a ControlChangeEvent only when a steal evicts a previous holder.
      if (evicts) broadcast();
      return create(ClaimTerminalControlResponseSchema, { granted: true, controlToken: token });
    },
    watchTerminalControl: async function* (
      req: { controlToken: string },
      context: { signal: AbortSignal },
    ) {
      const event = () =>
        create(TerminalControlEventSchema, {
          holderScreenId: lease.holderScreenId,
          youAreController: lease.token === req.controlToken,
        });
      yield event(); // snapshot
      while (!context.signal.aborted) {
        await Promise.race([
          nextBroadcast(),
          new Promise<void>((resolve) =>
            context.signal.addEventListener("abort", () => resolve(), { once: true }),
          ),
        ]);
        if (context.signal.aborted) break;
        yield event();
      }
    },
  });

  return { backend };
}

// ---------------------------------------------------------------------------
// Harness + fluent driver
// ---------------------------------------------------------------------------

function StealClaimHarness({
  client,
}: {
  client: ReturnType<typeof createClient<typeof ConnectionService>>;
}) {
  const session: Session = { sessionId: SESSION_ID, client };
  const { controlState, claim } = useTerminalControl(session, "fake-session-token");
  return (
    <>
      <span data-testid={TEST_IDS.controlIsControllerDisplay}>{String(controlState.isController)}</span>
      <span data-testid={TEST_IDS.controlHolderDisplay}>{controlState.holderScreenId}</span>
      <button data-testid={TEST_IDS.terminalClaimBtn} onClick={() => void claim()}>
        Claim terminal
      </button>
    </>
  );
}

function aStealClaimHarness() {
  const { backend } = aStealClaimDaemon();
  const client = createClient(ConnectionService, backend.transport());
  const isController = () => byTestId(TEST_IDS.controlIsControllerDisplay);

  return {
    mount() {
      cy.mount(<StealClaimHarness client={client} />);
      return this;
    },
    /** Assert the rendered controller flag settles to `value`. */
    expectIsController(value: boolean) {
      isController().should("have.text", String(value));
      return this;
    },
    clickClaim() {
      byTestId(TEST_IDS.terminalClaimBtn).click();
      return this;
    },
  };
}

// ---------------------------------------------------------------------------

it("becomes and stays the controller after a single steal-claim (no second Claim click needed)", () => {
  // Given — another screen holds the terminal-control lease, so this screen's auto-claim is denied
  // and it renders as a non-controller (the "Claim terminal" overlay would be shown over the terminal).
  const harness = aStealClaimHarness();
  harness.mount().expectIsController(false);

  // When — the user clicks "Claim terminal" exactly once.
  harness.clickClaim();

  // Then — this screen becomes the controller and STAYS the controller. The daemon's steal broadcast,
  // re-validated against the previously-denied subscription token, must not flip control back to
  // false (which is what forced the second click and left the terminal closed).
  harness.expectIsController(true);
});

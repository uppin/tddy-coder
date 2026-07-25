/**
 * Acceptance: the lazily-pulled Agent Activity transcript **survives host churn while it is in
 * flight**.
 *
 * The lazy snapshot is opened at most once per session, guarded by the registry's `snapshotOpened`
 * flag. That flag is set the moment the pull *starts*, so anything that tears the pull down before
 * its frames land leaves the session permanently marked "already pulled" with an empty transcript:
 * the badge keeps counting, the pane opens blank, and re-opening (or reloading) never recovers it.
 *
 * Two triggers are pinned here:
 *
 * - **A host that churns the client.** A new client identity re-runs the snapshot effect, whose
 *   cleanup cancels the in-flight pull. Production keeps that identity stable via
 *   `SessionClientCache` (see `AgentActivityClientIdentityAcceptance`), so this is the resilience
 *   case: even a host that does hand over a fresh client must still end up with the transcript.
 * - **A remount.** Deselecting/reselecting the session (or any chrome change that unmounts the
 *   header) cancels the pull the same way.
 *
 * The snapshot feed is held open by the backend until the spec releases it, so the pull is reliably
 * still in flight when the host churns — modelling the real network gap.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity.
 */

import React from "react";
import { createClient, type Transport } from "@connectrpc/connect";
import { ConnectionService } from "../../src/gen/connection_pb";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { agentChatPage } from "../support/pages/agentChatPage";
import { byTestId } from "../support/testIds";
import { aReplayBackendWithHeldSnapshot, replayAgentText } from "../support/rpc/acpReplay";

/** Driver for the test harnesses below — keeps raw selectors out of the test bodies. */
const hostPage = {
  /** Force the overlay's host to re-render, rebuilding the client it passes down. */
  rerender: () => byTestId("host-rerender").click(),
  /** Unmount the overlay, then mount it again for the same session. */
  toggleMounted: () => byTestId("host-toggle-mounted").click(),
};

/**
 * Harness mirroring `SessionMainPane`: the session-scoped `ConnectionService` client is built inline
 * during render, so every host render hands the overlay a brand-new client reference.
 */
function RerenderingHost({ transport }: { transport: Transport }) {
  const [renders, setRenders] = React.useState(0);
  const client = createClient(ConnectionService, transport);
  return (
    <div>
      <button data-testid="host-rerender" onClick={() => setRenders((n) => n + 1)}>
        rerender {renders}
      </button>
      <AgentActivityOverlay
        sessionId="in-flight-rerender"
        sessionToken="tok"
        sessionType="tool"
        client={client}
      />
    </div>
  );
}

/** Harness that can unmount and remount the overlay for one unchanged session. */
function RemountingHost() {
  const [mounted, setMounted] = React.useState(true);
  return (
    <div>
      <button data-testid="host-toggle-mounted" onClick={() => setMounted((m) => !m)}>
        toggle
      </button>
      {mounted && (
        <AgentActivityOverlay
          sessionId="in-flight-remount"
          sessionToken="tok"
          sessionType="tool"
        />
      )}
    </div>
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("renders the transcript when its host re-renders while the snapshot is still in flight", () => {
  // Given — one counted activity and a one-line transcript whose delivery is held open
  const { backend, opens, releaseSnapshot } = aReplayBackendWithHeldSnapshot({
    counts: [1],
    snapshot: [replayAgentText("Analyzing the parser.", 1_000)],
  });
  mountWithRpc(<RerenderingHost transport={backend.transport()} />, backend);

  // When — the operator opens the overlay (starting the held pull), the host re-renders with a
  // freshly-built client, and only then does the snapshot deliver its frames
  agentActivityPage.open();
  cy.wrap(opens).its("snapshot").should("equal", 1);
  hostPage.rerender();
  cy.then(() => releaseSnapshot());

  // Then — the transcript is rendered
  agentChatPage.chatMessage(0).should("have.text", "Analyzing the parser.");
});

it("renders the transcript after a remount cancelled the first in-flight snapshot", () => {
  // Given — one counted activity and a one-line transcript whose delivery is held open
  const { backend, opens, releaseSnapshot } = aReplayBackendWithHeldSnapshot({
    counts: [1],
    snapshot: [replayAgentText("Building the workspace.", 2_000)],
  });
  mountWithRpc(<RemountingHost />, backend);

  // When — the overlay is opened (starting the held pull), then unmounted and mounted again for the
  // same session, and the snapshot is released only after the remount
  agentActivityPage.open();
  cy.wrap(opens).its("snapshot").should("equal", 1);
  hostPage.toggleMounted();
  hostPage.toggleMounted();
  cy.then(() => releaseSnapshot());
  agentActivityPage.open();

  // Then — the remounted overlay shows the transcript rather than an empty pane
  agentChatPage.chatMessage(0).should("have.text", "Building the workspace.");
});

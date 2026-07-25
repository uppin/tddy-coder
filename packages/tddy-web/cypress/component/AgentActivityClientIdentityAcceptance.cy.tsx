/**
 * Acceptance: the Agent Activity feeds key on the **session-scoped client's identity**, and that
 * identity is **stable at the source**.
 *
 * Two halves of one contract, one test each:
 *
 * - **A host re-render must not churn the client.** Hosts build the session-scoped client inline
 *   while rendering (`buildSessionClient?.() ?? client` in `SessionMainPane`), so an
 *   unmemoized build hands the overlay a fresh client on every render. Resolving the build through
 *   `SessionClientCache` returns the *same* `Client` for an unchanged target, so the feeds stay on
 *   one subscription each.
 * - **A genuine transport change must be honored.** When a session's routing is upgraded
 *   (daemon-direct → session-scoped, once the session's own room connects), the client really does
 *   change, and the count feed must re-subscribe over the new transport rather than keep reporting
 *   from the old one.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity.
 */

import React from "react";
import { createClient, type Transport } from "@connectrpc/connect";
import { ConnectionService } from "../../src/gen/connection_pb";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { useSessionClientCache } from "../../src/components/sessions/sessionClientCache";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { agentChatPage } from "../support/pages/agentChatPage";
import { byTestId } from "../support/testIds";
import { aReplayBackend, replayAgentText } from "../support/rpc/acpReplay";

/** Driver for the harnesses below — keeps raw selectors out of the test bodies. */
const hostPage = {
  /** Force the overlay's host to re-render, rebuilding the client it passes down. */
  rerender: () => byTestId("host-rerender").click(),
  /** Swap the session's routing over to its session-scoped transport. */
  upgradeTransport: () => byTestId("host-upgrade-transport").click(),
};

/**
 * Harness mirroring `SessionMainPane`: the session-scoped client is rebuilt inline on every render,
 * but resolved through the production {@link SessionClientCache}, so an unchanged target yields one
 * stable client identity.
 */
function CachedClientHost({ transport }: { transport: Transport }) {
  const [renders, setRenders] = React.useState(0);
  const clientCache = useSessionClientCache();
  const client = clientCache.clientFor("daemon-i1-cached-session", transport, () =>
    createClient(ConnectionService, transport),
  );
  return (
    <div>
      <button data-testid="host-rerender" onClick={() => setRenders((n) => n + 1)}>
        rerender {renders}
      </button>
      <AgentActivityOverlay
        sessionId="cached-client"
        sessionToken="tok"
        sessionType="tool"
        client={client}
      />
    </div>
  );
}

/** Harness whose session routing is upgraded from a daemon-direct transport to a session-scoped one,
 *  handing the overlay a genuinely different client. */
function UpgradingHost({
  daemonDirect,
  sessionScoped,
}: {
  daemonDirect: Transport;
  sessionScoped: Transport;
}) {
  const [upgraded, setUpgraded] = React.useState(false);
  const transport = upgraded ? sessionScoped : daemonDirect;
  const client = React.useMemo(() => createClient(ConnectionService, transport), [transport]);
  return (
    <div>
      <button data-testid="host-upgrade-transport" onClick={() => setUpgraded(true)}>
        upgrade
      </button>
      <AgentActivityOverlay
        sessionId="upgrading-transport"
        sessionToken="tok"
        sessionType="tool"
        client={client}
      />
    </div>
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("keeps one subscription per feed while its host re-renders with an unchanged transport", () => {
  // Given — one counted activity and a one-line transcript, both already delivered
  const { backend, opens } = aReplayBackend({
    counts: [1],
    snapshot: [replayAgentText("Analyzing the parser.", 1_000)],
  });
  mountWithRpc(<CachedClientHost transport={backend.transport()} />, backend);
  agentActivityPage.open();
  agentChatPage.chatMessage(0).should("have.text", "Analyzing the parser.");

  // When — the host re-renders repeatedly, rebuilding the client each time
  hostPage.rerender();
  hostPage.rerender();
  hostPage.rerender();

  // Then — each feed was subscribed exactly once, and the transcript is untouched
  cy.wrap(opens).its("count").should("equal", 1);
  cy.wrap(opens).its("snapshot").should("equal", 1);
  agentChatPage.chatMessage(0).should("have.text", "Analyzing the parser.");
});

it("counts over the session-scoped transport once the session's routing is upgraded", () => {
  // Given — the daemon-direct route reports 2 activities, the session-scoped route reports 7
  const daemonDirect = aReplayBackend({ counts: [2] });
  const sessionScoped = aReplayBackend({ counts: [7] });
  mountWithRpc(
    <UpgradingHost
      daemonDirect={daemonDirect.backend.transport()}
      sessionScoped={sessionScoped.backend.transport()}
    />,
    daemonDirect.backend,
  );
  agentActivityPage.unreadBadge().should("have.text", "2");

  // When — the session's routing is upgraded to its own participant
  hostPage.upgradeTransport();

  // Then — the badge reports the count served by the upgraded transport
  agentActivityPage.unreadBadge().should("have.text", "7");
});

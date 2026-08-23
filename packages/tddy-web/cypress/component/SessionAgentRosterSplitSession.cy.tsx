/**
 * Acceptance: the Agent roster pane's catalog fan-out for a **split session** — one whose agent runs
 * on the daemon the browser is talking to while its codebase, worktree and roster live on another
 * host entirely.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI (AC49)
 *
 * `SessionAgentRosterPane.cy.tsx` covers the pane with the session's own host *and* the connected
 * host being the same machine, which is the ordinary case and the one where the fan-out's home is
 * ambiguous. A split session separates them, and the separation has consequences the ordinary case
 * cannot show:
 *
 *   • `ListSubagents` carries **no routing field** — a daemon answers it for its own defs and never
 *     forwards it — so the session's own host can only be asked by addressing that host directly;
 *   • the browser's HTTP transport reaches exactly **one** daemon, the one that served the bundle.
 *     Every other host, the session's included, exists for this browser only as a participant in the
 *     common room, and must therefore be reached over LiveKit RPC. There is no HTTP route to it: on a
 *     real deployment the session's host is a machine on another network with no reachable port.
 *
 * So each host answers from its **own** backend here, and the HTTP backend is the connected host's.
 * An option attributed to the session's host is proof the fan-out reached that host over LiveKit,
 * because nothing else could have produced it.
 */

import React from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionAgentRosterPane } from "../../src/components/sessions/SessionAgentRosterPane";
import { daemonRpcIdentity, type DaemonHost } from "../../src/lib/participantRole";
import {
  aDaemonOfferingAgents,
  aDaemonThatCannotBeReached,
  aSessionAgentRosterBackend,
  anAvailableAgent,
  type RosterBackend,
} from "../support/rpc/sessionAgentRosterBackend";
import { mountWithPerDaemonLiveKitRpc } from "../support/rpc/perDaemonLiveKitRpc";
import { withSelectedDaemonServedBy } from "../support/rpc/withSelectedDaemon";
import { sessionAgentRosterPage as page } from "../support/pages/sessionAgentRosterPage";

const SESSION_ID = "1780828020298-split";

/** The daemon that served this web bundle — the only host the browser's HTTP transport reaches. */
const CONNECTED_HOST: DaemonHost = { instanceId: "gateway-1", label: "gateway-1 (this daemon)" };
/**
 * The daemon facilitating the session: it holds the codebase, the worktree and the roster, and is
 * reachable from this browser only through the common room.
 */
const CODEBASE_HOST: DaemonHost = { instanceId: "codebase-2", label: "codebase-2" };

const EXPLORER_CONNECTED = "explorer@gateway-1";
const FASTCONTEXT_CODEBASE = "fastcontext@codebase-2";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/**
 * Mount the pane for a split session: the roster is owned by `CODEBASE_HOST` (which is what the
 * pane's `daemonInstanceId` names), while the browser's HTTP transport reaches `CONNECTED_HOST` and
 * serves `roster` — standing in for the connected daemon forwarding the session-scoped calls, which
 * it does because those requests carry a routing field.
 *
 * Returns the LiveKit target identities the mounted tree actually addressed, which is how a test
 * says "over LiveKit RPC" rather than merely "answered by that host's backend".
 */
function mountPaneForASplitSession(
  roster: RosterBackend,
  codebaseHost: InMemoryRpcBackend,
): { targets: string[] } {
  return mountWithPerDaemonLiveKitRpc(
    withSelectedDaemonServedBy(
      <SessionAgentRosterPane
        sessionId={SESSION_ID}
        sessionToken="tok"
        daemonInstanceId={CODEBASE_HOST.instanceId}
        daemonConnected
      />,
      [CONNECTED_HOST, CODEBASE_HOST],
      CONNECTED_HOST.instanceId,
    ),
    { [daemonRpcIdentity(CODEBASE_HOST.instanceId)]: codebaseHost },
    { httpBackend: roster.backend },
  );
}

/** The roster the connected daemon serves for this session, plus the agents it offers itself. */
function aRosterServedOverHttp(offers: ReturnType<typeof anAvailableAgent>[]): RosterBackend {
  return aSessionAgentRosterBackend({
    sessionId: SESSION_ID,
    initial: [],
    rev: 0,
    offers,
  });
}

describe("Agent roster pane for a split session", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("offers an agent that only the session's codebase host defines", () => {
    // Given — the codebase host defines fastcontext; the connected host has never heard of it
    const roster = aRosterServedOverHttp([anAvailableAgent("explorer", CONNECTED_HOST.instanceId)]);
    mountPaneForASplitSession(
      roster,
      aDaemonOfferingAgents([anAvailableAgent("fastcontext", CODEBASE_HOST.instanceId, ["Grep"])]),
    );

    // When
    page.openPicker();

    // Then — on offer under its qualified id, attributed to the host that defines it
    page.pickerOption(FASTCONTEXT_CODEBASE).should("exist");
    page.pickerOptionHost(FASTCONTEXT_CODEBASE).should("have.text", CODEBASE_HOST.instanceId);
  });

  it("addresses the session's codebase host over LiveKit RPC, and no other host", () => {
    // Given
    const roster = aRosterServedOverHttp([anAvailableAgent("explorer", CONNECTED_HOST.instanceId)]);
    const { targets } = mountPaneForASplitSession(
      roster,
      aDaemonOfferingAgents([anAvailableAgent("fastcontext", CODEBASE_HOST.instanceId)]),
    );

    // When
    page.openPicker();
    page.pickerOption(FASTCONTEXT_CODEBASE).should("exist");

    // Then — the codebase host was reached over the common room, and the connected host was read
    // over the transport the browser already holds rather than addressed as a peer
    cy.wrap(null).should(() => {
      expect(targets).to.deep.equal([daemonRpcIdentity(CODEBASE_HOST.instanceId)]);
    });
  });

  it("keeps the connected host's agents on offer when the codebase host cannot be reached", () => {
    // Given — the common room holds the codebase host, but it does not answer
    const roster = aRosterServedOverHttp([
      anAvailableAgent("explorer", CONNECTED_HOST.instanceId, ["Grep"]),
    ]);
    mountPaneForASplitSession(roster, aDaemonThatCannotBeReached("codebase-2 is not reachable"));

    // When
    page.openPicker();

    // Then — one error row naming the host an operator would go and look at, never the picker
    page.pickerHostError(CODEBASE_HOST.instanceId).should("contain.text", "codebase-2 is not reachable");
    page.pickerOption(EXPLORER_CONNECTED).should("exist");
  });

  it("blames the connected host, not the session's codebase host, for a catalog read it failed", () => {
    // Given — the daemon this browser is connected to serves the roster but cannot list its own defs
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      rev: 0,
      offersUnavailable: "gateway-1 has no agents directory",
    });
    mountPaneForASplitSession(
      roster,
      aDaemonOfferingAgents([anAvailableAgent("fastcontext", CODEBASE_HOST.instanceId)]),
    );

    // When
    page.openPicker();

    // Then — the failure is attributed to the host that failed, and the codebase host answered fine
    page
      .pickerHostError(CONNECTED_HOST.instanceId)
      .should("contain.text", "gateway-1 has no agents directory");
    page.pickerHostError(CODEBASE_HOST.instanceId).should("not.exist");
  });
});

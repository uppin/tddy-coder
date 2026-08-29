/**
 * Acceptance: which half of a **split session** the Session Inspector's Agents tab talks to.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI, docs/ft/daemon/remote-managed-worktree.md
 *
 * A split session is two sessions: the agent runs on one host, its worktree and codebase live on
 * another. Only one of the two halves keeps the roster — the codebase half — and it is the roster
 * the agent's own tooling reads to know which tools were taken away from it. A drawer that asked the
 * agent half would be answered, and answered with a roster nothing enforces: it would show no agents
 * where agents are attached, and an attach made there would report a withdrawal no process performs.
 *
 * `SessionAgentRosterSplitSession.cy.tsx` covers the pane once it has been told which host owns the
 * roster. This spec covers the step before that — the drawer deciding what to tell it.
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { ConnectionService, type SessionEntry } from "../../src/gen/connection_pb";
import { SessionInspectorDrawer } from "../../src/components/sessions/SessionInspectorDrawer";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionAgentRosterBackend,
  anAvailableAgent,
  type RosterBackend,
} from "../support/rpc/sessionAgentRosterBackend";
import { sessionAgentRosterPage as page } from "../support/pages/sessionAgentRosterPage";

const SESSION_TOKEN = "tok-inspector-roster";

/** The host running the agent — the daemon this browser is talking to, and the drawer's session. */
const AGENT_HOST = "gateway-1";
/** The host holding the worktree, and with it the roster that governs the agent's tools. */
const CODEBASE_HOST = "codebase-2";

const AGENT_SESSION_ID = "1780828020298-agent";
const CODEBASE_SESSION_ID = "1780828020311-codebase";

const FASTCONTEXT = `fastcontext@${CODEBASE_HOST}`;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The agent half of a split session, as `ListSessions` on the agent host reports it. */
const SPLIT_SESSION = {
  sessionId: AGENT_SESSION_ID,
  createdAt: "2026-08-29T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-split",
  pid: 8181,
  isActive: true,
  projectId: "proj-split-1",
  daemonInstanceId: AGENT_HOST,
  codebaseDaemonInstanceId: CODEBASE_HOST,
  codebaseSessionId: CODEBASE_SESSION_ID,
  pendingElicitation: false,
};

/** An ordinary session: one host runs the agent and holds the codebase, so it keeps its own roster. */
const CO_LOCATED_SESSION = {
  ...SPLIT_SESSION,
  sessionId: "1780828020400-colocated",
  codebaseDaemonInstanceId: "",
  codebaseSessionId: "",
};

/** The roster of the codebase half, plus the agent that host offers. */
function aRosterOnTheCodebaseHost(): RosterBackend {
  return aSessionAgentRosterBackend({
    sessionId: CODEBASE_SESSION_ID,
    initial: [],
    rev: 0,
    offers: [anAvailableAgent("fastcontext", CODEBASE_HOST, ["Grep"])],
  });
}

function mountInspectorOn(session: typeof SPLIT_SESSION, roster: RosterBackend) {
  const client = createClient(ConnectionService, roster.backend.transport());
  const noop = () => undefined;
  mountWithRpc(
    <SessionInspectorDrawer
      state="open"
      session={session as unknown as SessionEntry}
      onClose={noop}
      onExpand={noop}
      onRestore={noop}
      onResume={noop}
      onDelete={noop}
      onTerminate={noop}
      client={client}
      sessionToken={SESSION_TOKEN}
    />,
    roster.backend,
  );
}

describe("Session Inspector — the Agents tab of a split session", () => {
  it("reads the roster from the codebase half, which is the half that keeps it", () => {
    // Given — the inspector is open on the agent half of a split session
    const roster = aRosterOnTheCodebaseHost();
    mountInspectorOn(SPLIT_SESSION, roster);

    // When — the Agents tab is selected
    page.openInspectorAgentsTab();

    // Then — the roster was read from the codebase host, under the codebase half's session id
    page.pane().should("be.visible");
    cy.wrap(roster).should((r: RosterBackend) => {
      expect(r.rosterReadsAddressed()).to.deep.equal([
        { sessionId: CODEBASE_SESSION_ID, daemonInstanceId: CODEBASE_HOST },
      ]);
    });
  });

  it("attaches an agent to the codebase half, so the withdrawal it reports is the one enforced", () => {
    // Given — the inspector is open on the Agents tab of that same split session
    const roster = aRosterOnTheCodebaseHost();
    mountInspectorOn(SPLIT_SESSION, roster);
    page.openInspectorAgentsTab();
    page.pane().should("be.visible");

    // When — an agent the codebase host offers is picked and added
    page.openPicker();
    page.selectInPicker(FASTCONTEXT);
    page.confirmAttach();

    // Then — the attach named the codebase half
    cy.wrap(roster).should((r: RosterBackend) => {
      expect(r.attachesAddressed()).to.deep.equal([
        { sessionId: CODEBASE_SESSION_ID, daemonInstanceId: CODEBASE_HOST },
      ]);
    });
  });

  it("reads a co-located session's roster from the session's own host", () => {
    // Given — the inspector is open on a session whose agent and codebase share a host
    const roster = aSessionAgentRosterBackend({
      sessionId: CO_LOCATED_SESSION.sessionId,
      initial: [],
      rev: 0,
    });
    mountInspectorOn(CO_LOCATED_SESSION, roster);

    // When — the Agents tab is selected
    page.openInspectorAgentsTab();

    // Then — nothing was redirected: the session's own ids addressed the roster
    page.pane().should("be.visible");
    cy.wrap(roster).should((r: RosterBackend) => {
      expect(r.rosterReadsAddressed()).to.deep.equal([
        { sessionId: CO_LOCATED_SESSION.sessionId, daemonInstanceId: AGENT_HOST },
      ]);
    });
  });
});

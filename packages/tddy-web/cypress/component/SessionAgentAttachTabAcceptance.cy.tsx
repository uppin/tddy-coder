/**
 * Acceptance: the session header's "Add agent" attaches a roster agent to the session the operator
 * is already looking at, and opens a conversation tab with it.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-29-session-agent-conversation-tab.md (AC1-AC4, AC10-AC12)
 * Changeset: docs/dev/1-WIP/CS-2026-08-29-session-agent-conversation-tab.md
 *
 * Driven through `SessionsDrawerScreen` rather than through `SessionMainPane` alone, because the
 * property under test spans three collaborators that a narrower mount would let disagree silently:
 * the header button lives in `SessionMainPane`, the attach and open are RPCs, and the tab it opens
 * is rendered by `SessionRuntime` — which only exists once a session is attached.
 *
 * The session attaches over the deterministic gRPC path (`connectSession` returns an empty
 * `livekitRoom`), so every RPC lands in the in-memory backend, as in
 * `SessionChildTabsAcceptance.cy.tsx`.
 *
 * This spec replaces `SessionsDrawerPeerSpawn.cy.tsx`: the same button used to call `StartSession`
 * with an `orchestrator_session_id` and spawn a whole peer *session*. The last case below is what
 * pins that it no longer does.
 */

import React from "react";
import { ConnectionService } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { anAvailableAgent } from "../support/rpc/sessionAgentRosterBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";
import { sessionAgentConversationPage as page } from "../support/pages/sessionAgentConversationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST = "local";
const EXPLORER = "explorer@local";
const LINTER = "linter@local";

/** A cursor-cli session: a genuine PTY session, so it renders the terminal runtime that hosts the
 *  tab strip. A `tool` session with a recipe opens the full-screen workflow chat instead and has no
 *  tab strip at all, which is why the type is stated rather than left to the proto default. */
const SESSION = {
  sessionId: "agent-tab-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-08-29T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-agent-tab-1",
  daemonInstanceId: HOST,
  sessionType: "cursor-cli",
  pendingElicitation: false,
};

/**
 * A connected-grpc backend whose daemon offers `offers` in the picker, holds an empty roster, and
 * answers the conversation RPCs with `answer`.
 */
function aBackendOffering(
  offers: ReturnType<typeof anAvailableAgent>[],
  answer: string[] = ["ready when you are"],
): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    sessionAgents: { sessionId: SESSION.sessionId, initial: [], rev: 0, offers },
    agentConversations: { answerChunks: answer, stopReason: "EndTurn" },
  });
}

/** The same session, but its daemon refuses to open a conversation with the attached agent. */
function aBackendThatCannotOpen(
  offers: ReturnType<typeof anAvailableAgent>[],
  message: string,
): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    sessionAgents: { sessionId: SESSION.sessionId, initial: [], rev: 0, offers },
    agentConversations: { openFails: message },
  });
}

/** Attach the session and wait for its tab strip — every case starts here. */
function attachSession(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  tabs.tabs().should("exist");
}

// ---------------------------------------------------------------------------

describe("Add agent — attach a roster agent and open a conversation with it", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC1, AC2 — the picker, and what it says the attach costs
  // -------------------------------------------------------------------------

  it("offers the daemon's agents when Add agent is clicked", () => {
    // Given a session whose host offers two agents
    attachSession(
      aBackendOffering([anAvailableAgent("explorer", HOST), anAvailableAgent("linter", HOST)]),
    );

    // When the operator opens the picker
    page.openPicker();

    // Then exactly those two are on offer, under their qualified ids — an existence check apiece
    // would pass against a picker that also offered an agent this host never named
    page.assertPickerOffers([EXPLORER, LINTER]);
  });

  it("says which tools the main agent loses before the operator confirms", () => {
    // Given an agent that would take Bash and WebFetch from the main agent. Deliberately disjoint
    // from the builder's default `tools` (`Read`/`Glob`/`Grep`): an overlapping set would let this
    // pass against a picker that rendered the agent's own tools instead of what it replaces.
    attachSession(aBackendOffering([anAvailableAgent("explorer", HOST, ["Bash", "WebFetch"])]));

    // When it is highlighted in the picker
    page.openPicker();
    page.selectInPicker(EXPLORER);

    // Then the cost is stated in full, before anything is sent
    page
      .pickerWithdrawalWarning()
      .should(
        "have.text",
        `The main agent loses Bash, WebFetch while ${EXPLORER} is attached.`,
      );
  });

  // -------------------------------------------------------------------------
  // AC3 — the attach itself
  // -------------------------------------------------------------------------

  it("attaches the picked agent to the current session under its qualified id", () => {
    // Given a session offering one agent
    const backend = aBackendOffering([anAvailableAgent("explorer", HOST)]);
    attachSession(backend);

    // When the operator attaches it
    page.attachAgent(EXPLORER);

    // Then the attach names this session on this session's daemon — an agent attached to the wrong
    // session is the one failure this flow could not show on screen
    cy.wrap(null).should(() => {
      expect(backend.attachedAgentIds()).to.deep.equal([EXPLORER]);
      expect(backend.attachesAddressed()).to.deep.equal([
        { sessionId: SESSION.sessionId, daemonInstanceId: HOST },
      ]);
    });
  });

  it("does not start a session when an agent is attached", () => {
    // Given a session offering one agent
    const backend = aBackendOffering([anAvailableAgent("explorer", HOST)]);
    attachSession(backend);

    // When the operator attaches it
    page.attachAgent(EXPLORER);
    page.tabs().should("have.length", 1);

    // Then no peer session was spawned — this is the behaviour the flow replaced
    cy.wrap(backend).should((b: ConnectionServiceBackend) => {
      expect(b.callsTo(ConnectionService.method.startSession)).to.have.length(0);
    });
  });

  // -------------------------------------------------------------------------
  // AC4 — the tab it opens
  // -------------------------------------------------------------------------

  it("opens a conversation with the attached agent and focuses its tab", () => {
    // Given a session offering one agent
    const backend = aBackendOffering([anAvailableAgent("explorer", HOST)]);
    attachSession(backend);

    // When the operator attaches it
    page.attachAgent(EXPLORER);

    // Then a conversation is opened with that agent ...
    cy.wrap(null).should(() => {
      expect(backend.openedConversations().map((c) => c.agentId)).to.deep.equal([EXPLORER]);
    });
    // ... and its tab is the one showing, with the Agent terminal stepped aside
    page.tabs().should("have.length", 1);
    page.tabs().should("have.attr", "aria-selected", "true");
    tabs.agentTab().should("have.attr", "aria-selected", "false");
    page.panes().should("have.length", 1).and("be.visible");
  });

  it("keys the tab by the conversation the daemon was asked to open", () => {
    // Given an attached agent
    const backend = aBackendOffering([anAvailableAgent("explorer", HOST)]);
    attachSession(backend);
    page.attachAgent(EXPLORER);
    page.tabs().should("have.length", 1);

    // When the conversation the browser opened is read back
    // Then the tab on screen is keyed by that very id — a tab keyed by anything else could not
    // cancel the conversation it claims to hold
    cy.then(() => {
      const [opened] = backend.openedConversations();
      page.tab(opened.conversationId).should("exist");
      page.pane(opened.conversationId).should("be.visible");
    });
  });

  it("carries the operator's prompt to the attached agent and shows its answer", () => {
    // Given an attached agent that answers
    attachSession(
      aBackendOffering([anAvailableAgent("explorer", HOST)], ["I read ", "three files"]),
    );
    page.attachAgent(EXPLORER);
    page.tabs().should("have.length", 1);

    // When the operator asks it something
    page.prompt("what have you read?");

    // Then the exchange is in the tab
    page.assertTurn(0, "operator", "what have you read?");
    page.assertTurn(1, "agent", "I read three files");
  });

  // -------------------------------------------------------------------------
  // AC11 — attaching the same agent twice
  // -------------------------------------------------------------------------

  it("focuses the existing tab when the same agent is attached again", () => {
    // Given an agent already attached, with a conversation open
    const backend = aBackendOffering([anAvailableAgent("explorer", HOST)]);
    attachSession(backend);
    page.attachAgent(EXPLORER);
    page.tabs().should("have.length", 1);

    // When the operator attaches the same agent a second time
    page.attachAgent(EXPLORER);

    // Then there is still one tab: the second attach was a no-op on the roster, so a second tab
    // would claim something the daemon did not do
    page.tabs().should("have.length", 1);
    page.tabs().should("have.attr", "aria-selected", "true");
  });

  it("opens a second tab for a second, different agent", () => {
    // Given a host offering two agents, one of them already attached
    const backend = aBackendOffering([
      anAvailableAgent("explorer", HOST),
      anAvailableAgent("linter", HOST),
    ]);
    attachSession(backend);
    page.attachAgent(EXPLORER);
    page.tabs().should("have.length", 1);

    // When the second is attached too
    page.attachAgent(LINTER);

    // Then both conversations are open
    page.tabs().should("have.length", 2);
    cy.wrap(null).should(() => {
      expect(backend.openedConversations().map((c) => c.agentId)).to.deep.equal([
        EXPLORER,
        LINTER,
      ]);
    });
  });

  // -------------------------------------------------------------------------
  // AC10 — closing the tab
  // -------------------------------------------------------------------------

  it("cancels the conversation and returns focus to the Agent tab when its tab is closed", () => {
    // Given an open conversation
    const backend = aBackendOffering([anAvailableAgent("explorer", HOST)]);
    attachSession(backend);
    page.attachAgent(EXPLORER);
    page.tabs().should("have.length", 1);

    // When its tab is closed
    cy.then(() => {
      const [opened] = backend.openedConversations();
      page.closeTab(opened.conversationId);

      // Then exactly the conversation that tab held is cancelled — a count alone would pass while
      // the wrong conversation was dropped, which is the failure this flow could not show on screen
      cy.wrap(null).should(() => {
        expect(backend.cancelledConversationIds()).to.deep.equal([opened.conversationId]);
      });
    });
    page.tabs().should("not.exist");
    tabs.agentTab().should("have.attr", "aria-selected", "true");
  });

  it("does not cancel a conversation the daemon refused to open", () => {
    // Given an attached agent whose clone is not ready, so the open is refused
    const backend = aBackendThatCannotOpen(
      [anAvailableAgent("explorer", HOST)],
      "explorer@local is still provisioning",
    );
    attachSession(backend);
    page.attachAgent(EXPLORER);
    page.error().should("contain.text", "explorer@local is still provisioning");

    // When the operator closes the tab. It is closed by position rather than by id: the refused open
    // is exactly the case where no id was ever recorded to close it by.
    page.closeOnlyTab();

    // Then nothing is cancelled — a refused open created no conversation, and asking the daemon to
    // drop one it never had is a request that can only ever fail
    page.tabs().should("not.exist");
    cy.wrap(null).should(() => {
      expect(backend.cancelledConversationIds()).to.deep.equal([]);
    });
  });

  // -------------------------------------------------------------------------
  // AC12 — the peer-spawn flow is gone
  // -------------------------------------------------------------------------

  it("does not open the session-creation pane when Add agent is clicked", () => {
    // Given a session offering one agent
    attachSession(aBackendOffering([anAvailableAgent("explorer", HOST)]));

    // When the operator clicks Add agent
    page.attachBtn().click();

    // Then the button offers a roster attach, not the peer-session creation form it used to open
    page.picker().should("be.visible");
    sessionsDrawerPage.createSessionPane().should("not.exist");
  });
});

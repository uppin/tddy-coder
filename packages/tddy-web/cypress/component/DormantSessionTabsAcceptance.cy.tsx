/**
 * Cypress component acceptance: a stopped session keeps its tab strip.
 *
 * A session whose agent is not running (`isActive: false`) used to lose its whole runtime — tab
 * strip, "+" and agent-conversation tabs — behind the full-pane recorded transcript. It now shows the
 * strip: the Agent tab holds the transcript, "+" opens terminals through the owning daemon, the
 * daemon's existing tabs are listed, and an added agent's conversation gets its tab.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-session-tabs.md (`#session-tabs 1/7`)
 *
 * Driven over the in-memory daemon: terminal RPCs, the recorded transcript and the agent roster are
 * all served by `aSessionServiceBackend`.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { anAvailableAgent } from "../support/rpc/sessionAgentRosterBackend";
import { replayAgentText } from "../support/rpc/acpReplay";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";
import { sessionAgentConversationPage as conversations } from "../support/pages/sessionAgentConversationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST = "local";
const EXPLORER = "explorer@local";

/** A stopped claude-cli session: a PTY session whose agent process has exited. */
const STOPPED = {
  sessionId: "stopped-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-11-02T09:00:00Z",
  status: "exited",
  repoPath: "/home/dev/finished-work",
  pid: 0,
  isActive: false,
  projectId: "proj-stopped-tabs-1",
  daemonInstanceId: HOST,
  sessionType: "claude-cli",
  pendingElicitation: false,
};

/** The transcript the daemon replays for the stopped session. */
const RECORDED_TRANSCRIPT = {
  counts: [1],
  snapshot: [replayAgentText("Rewrote the tokenizer.", 1_000)],
  details: {},
};

/** A daemon serving the stopped session, its transcript, an agent to add, and `terminals`. */
function aStoppedSessionBackend(
  terminals: Array<{ terminalId: string }> = [],
): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [STOPPED],
    terminals,
    acpReplay: RECORDED_TRANSCRIPT,
    sessionAgents: {
      sessionId: STOPPED.sessionId,
      initial: [],
      rev: 0,
      offers: [anAvailableAgent("explorer", HOST)],
    },
    agentConversations: { answerChunks: ["ready"], stopReason: "EndTurn" },
  });
}

/**
 * The same session, reported live by every `ListSessions` after a `ResumeSession` — the daemon's
 * view once the agent is running again. Served host-direct (empty room), like any live PTY session.
 */
function aStoppedSessionThatResumes(): SessionServiceBackend {
  let resumed = false;
  return aSessionServiceBackend({
    listSessionsFactory: () =>
      resumed ? [{ ...STOPPED, status: "active", pid: 4242, isActive: true }] : [STOPPED],
    acpReplay: RECORDED_TRANSCRIPT,
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    resumeSession: (sessionId) => {
      resumed = true;
      return { sessionId, livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };
    },
  });
}

function selectStoppedSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(STOPPED.sessionId).click();
}

// ---------------------------------------------------------------------------

describe("A stopped session keeps its tab strip", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("a stopped session shows its tab strip with the recorded transcript in the Agent tab", () => {
    // Given a stopped session with a recorded transcript
    const backend = aStoppedSessionBackend();

    // When it is selected
    selectStoppedSession(backend);

    // Then its strip is shown, the Agent tab is selected, and its pane holds the transcript
    tabs.tabs().should("be.visible");
    tabs.agentTab().should("have.attr", "aria-selected", "true");
    tabs.newTab().should("be.visible");
    tabs.pane("main").within(() => {
      sessionActivitiesPage.pane().should("contain.text", "Rewrote the tokenizer.");
    });
  });

  it('"+" → Bash on a stopped session opens a working terminal tab', () => {
    // Given a selected stopped session
    const backend = aStoppedSessionBackend();
    selectStoppedSession(backend);

    // When the operator opens a bash terminal
    tabs.newTab().click();
    tabs.newTabOption("bash").click();

    // Then the daemon was asked to start it for this session, and its tab streams that terminal
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startTerminalSessionIds).to.deep.equal([STOPPED.sessionId]);
    });
    tabs.tab("bash-1").should("have.attr", "aria-selected", "true");
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.streamedTerminals.map((s) => s.terminalId)).to.include("bash-1");
    });
  });

  it("a stopped session lists the terminals the daemon already runs for it", () => {
    // Given a stopped session whose daemon still runs a bash tab for it
    const backend = aStoppedSessionBackend([{ terminalId: "bash-1" }]);

    // When it is selected
    selectStoppedSession(backend);

    // Then that tab is in the strip
    tabs.tab("bash-1").should("exist");
  });

  it("Add agent on a stopped session opens a conversation tab in its strip", () => {
    // Given a selected stopped session offering one agent
    const backend = aStoppedSessionBackend();
    selectStoppedSession(backend);
    tabs.tabs().should("be.visible");

    // When the operator adds it
    conversations.attachAgent(EXPLORER);

    // Then a conversation is opened and its tab is the one showing
    cy.wrap(null).should(() => {
      expect(backend.openedConversations().map((c) => c.agentId)).to.deep.equal([EXPLORER]);
    });
    conversations.tabs().should("have.length", 1).and("have.attr", "aria-selected", "true");
    conversations.panes().should("have.length", 1).and("be.visible");
  });

  it("a tab opened while the session was stopped is still in the strip after Resume", () => {
    // Given a stopped session with a bash tab opened while it was stopped
    const backend = aStoppedSessionThatResumes();
    selectStoppedSession(backend);
    tabs.newTab().click();
    tabs.newTabOption("bash").click();
    tabs.tab("bash-1").should("exist");

    // When the operator resumes the session
    sessionsDrawerPage.detailResumeBtn(STOPPED.sessionId).click();

    // Then the live session's strip still carries that tab
    sessionsDrawerPage.detailResumeBtn(STOPPED.sessionId).should("not.exist");
    tabs.tab("bash-1").should("exist");
  });
});

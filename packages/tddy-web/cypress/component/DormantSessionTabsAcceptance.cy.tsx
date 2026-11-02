/**
 * Cypress component acceptance: a stopped session keeps its tab strip.
 *
 * A session whose agent is not running (`isActive: false`) used to lose its whole runtime — tab
 * strip, "+" and agent-conversation tabs — behind the full-pane recorded transcript. It now shows the
 * strip: the Agent tab holds the transcript, "+" opens terminals through the owning daemon, the
 * daemon's existing tabs are listed, and an added agent's conversation gets its tab.
 *
 * Feature: docs/ft/web/session-terminal-tabs.md § Stopped sessions
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
import { sessionFilesTabPage as files } from "../support/pages/sessionFilesTabPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";
import { sessionAgentConversationPage as conversations } from "../support/pages/sessionAgentConversationPage";
import { byTestId } from "../support/testIds";

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
  agent: "claude-cli",
  pendingElicitation: false,
};

/** Another stopped session, to move the selection to and come back from. */
const ANOTHER = {
  ...STOPPED,
  sessionId: "stopped-tabs-bbbbbbbb-0000-0000-0000-000000000002",
  repoPath: "/home/dev/other-work",
  projectId: "proj-stopped-tabs-2",
};

/** A file already uploaded to the stopped session. */
const UPLOAD = {
  uploadId: "up-1",
  fileName: "report.pdf",
  hostPath: "/home/dev/finished-work/uploads/report.pdf",
  sizeBytes: 1024n,
  uploadedAtMs: 1_700_000_000_000n,
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
  extra: { leaseHeldBy?: string } = {},
): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [STOPPED, ANOTHER],
    terminals,
    uploads: [UPLOAD],
    ...extra,
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
    sessionAgents: {
      sessionId: STOPPED.sessionId,
      initial: [],
      rev: 0,
      offers: [anAvailableAgent("explorer", HOST)],
    },
    agentConversations: { answerChunks: ["ready"], stopReason: "EndTurn" },
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

/**
 * Stub the Fullscreen API on the AUT window and drive `document.fullscreenElement` from the stub, so
 * the component's `fullscreenchange` listener sees the state the browser would have produced (as
 * `SessionTerminalFullscreenAcceptance` does). Must run AFTER mount.
 */
function stubFullscreenApi() {
  cy.window().then((win) => {
    const setActive = (el: Element | null) => {
      Object.defineProperty(win.document, "fullscreenElement", {
        configurable: true,
        get: () => el,
      });
      win.document.dispatchEvent(new win.Event("fullscreenchange"));
    };
    cy.stub(win.Element.prototype, "requestFullscreen")
      .as("requestFullscreen")
      .callsFake(function (this: Element) {
        setActive(this);
        return Promise.resolve();
      });
    cy.stub(win.document, "exitFullscreen")
      .as("exitFullscreen")
      .callsFake(() => {
        setActive(null);
        return Promise.resolve();
      });
    setActive(null);
  });
}

// ---------------------------------------------------------------------------

describe("A stopped session keeps its tab strip", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("a stopped session shows its tab strip with the greyed recorded output in the Agent tab", () => {
    // Given a stopped session with a recorded transcript
    const backend = aStoppedSessionBackend();

    // When it is selected
    selectStoppedSession(backend);

    // Then its strip is shown, the Agent tab is selected, and its pane is the greyed recorded output
    tabs.tabs().should("be.visible");
    tabs.agentTab().should("have.attr", "aria-selected", "true");
    tabs.newTab().should("be.visible");
    tabs.pane("main").within(() => {
      byTestId("dormant-agent-terminal").should("have.attr", "data-color-scheme", "history");
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
    sessionActivitiesPage.resume(STOPPED.sessionId);

    // Then the live session's strip still carries that tab
    sessionActivitiesPage.resumeBtn(STOPPED.sessionId).should("not.exist");
    tabs.tab("bash-1").should("exist");
  });

  it("a conversation opened on a stopped session is not cancelled by leaving it and coming back", () => {
    // Given a conversation opened on a stopped session
    const backend = aStoppedSessionBackend();
    selectStoppedSession(backend);
    conversations.attachAgent(EXPLORER);
    conversations.tabs().should("have.length", 1).and("have.attr", "aria-selected", "true");

    // When another session is selected and this one is selected again
    sessionsDrawerPage.drawerItem(ANOTHER.sessionId).click();
    sessionsDrawerPage.drawerItem(STOPPED.sessionId).click();

    // Then the conversation was never cancelled, and it is still the tab holding the pane
    cy.wrap(null).should(() => {
      expect(backend.cancelledConversationIds()).to.deep.equal([]);
    });
    conversations.tabs().should("have.length", 1).and("have.attr", "aria-selected", "true");
    conversations.panes().should("have.length", 1).and("be.visible");
  });

  it("a conversation opened on a stopped session is not cancelled by Resume", () => {
    // Given a conversation opened on a stopped session
    const backend = aStoppedSessionThatResumes();
    selectStoppedSession(backend);
    conversations.attachAgent(EXPLORER);
    conversations.tabs().should("have.length", 1).and("have.attr", "aria-selected", "true");

    // When the operator resumes the session
    sessionActivitiesPage.resume(STOPPED.sessionId);
    sessionActivitiesPage.resumeBtn(STOPPED.sessionId).should("not.exist");

    // Then the conversation was never cancelled, and it is still the open tab
    cy.wrap(null).should(() => {
      expect(backend.cancelledConversationIds()).to.deep.equal([]);
    });
    conversations.tabs().should("have.length", 1).and("have.attr", "aria-selected", "true");
    conversations.panes().should("have.length", 1).and("be.visible");
  });

  it("a stopped session's terminal can be taken to full screen", () => {
    // Given a stopped session with its strip showing
    const backend = aStoppedSessionBackend();
    selectStoppedSession(backend);
    tabs.tabs().should("be.visible");
    stubFullscreenApi();

    // When the operator asks for full screen
    tabs.fullscreenToggle().click();

    // Then this session's pane stack — not its tab strip — is handed to the Fullscreen API, and the
    // pane draws its own way back
    tabs.paneStack(STOPPED.sessionId).then(($stack) => {
      cy.get("@requestFullscreen").should((subject) => {
        const stub = subject as unknown as sinon.SinonStub;
        expect(stub.firstCall.thisValue).to.equal($stack[0]);
      });
    });
    tabs.fullscreenExit().should("exist").click({ force: true });
    cy.get("@exitFullscreen").should("have.been.calledOnce");
  });

  it("a stopped session's bash tab carries the tool's shortcut drawer", () => {
    // Given a stopped claude-cli session with a bash tab
    const backend = aStoppedSessionBackend([{ terminalId: "bash-1" }]);
    selectStoppedSession(backend);

    // When the bash tab is the one showing
    tabs.tab("bash-1").click();

    // Then its pane carries the shortcut drawer for the session's tool
    tabs.paneShortcutDrawer("bash-1").should("exist");
  });

  it("the Files tab inserts an uploaded path into a stopped session's focused bash tab", () => {
    // Given a stopped session with a bash tab showing, and an uploaded file
    const backend = aStoppedSessionBackend([{ terminalId: "bash-1" }]);
    selectStoppedSession(backend);
    tabs.tab("bash-1").click();
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.streamedTerminals.map((s) => s.terminalId)).to.include("bash-1");
    });

    // When the file's Insert is pressed in the inspector's Files tab
    sessionsDrawerPage.inspectorToggle().click();
    files.tabButton().click();
    files.insert(UPLOAD.fileName).click();

    // Then the shell-quoted path was typed into that bash terminal
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      const typed = b.sentTerminalInput
        .filter((i) => i.terminalId === "bash-1")
        .map((i) => new TextDecoder().decode(i.data))
        .join("");
      expect(typed).to.contain(UPLOAD.hostPath);
    });
  });

  it("a stopped session's bash tab offers Claim terminal while another screen holds its lease", () => {
    // Given a stopped session whose terminals' lease another screen holds, with a bash tab
    const backend = aStoppedSessionBackend([{ terminalId: "bash-1" }], {
      leaseHeldBy: "screen-held-by-another-9999",
    });
    selectStoppedSession(backend);

    // Then the Agent tab (a transcript, which takes no input) offers no claim
    tabs.agentTab().should("have.attr", "aria-selected", "true");
    sessionsDrawerPage.terminalControlOverlay({ timeout: 1000 }).should("not.exist");

    // When the bash tab is shown, the toast names the holder and offers the claim
    tabs.tab("bash-1").click();
    sessionsDrawerPage.terminalControlHolder().should("contain.text", "screen-held-by-another-9999");

    // And claiming takes the lease, which removes the toast
    sessionsDrawerPage.terminalClaimBtn().click();
    sessionsDrawerPage.terminalControlOverlay({ timeout: 1000 }).should("not.exist");
  });
});

/**
 * Cypress component acceptance: Session terminal tabs — Agent + multiple bash terminals per session.
 *
 * PRD: `docs/ft/web/session-terminal-tabs.md`
 * Changeset: `session-terminal-tabs`
 *
 * Driven over the deterministic gRPC path: `connectSession` returns an empty `livekitRoom`, so the
 * session attaches as `connected-grpc` and every terminal RPC (List/Start/Stop/StreamOutput/
 * SendInput) flows over the daemon client into the in-memory backend.
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
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixture — a single active local session attached over gRPC
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "term-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-15T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-term-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

/** A connected-grpc backend (empty `livekitRoom`) with an optional set of pre-existing bash tabs. */
function aGrpcBackend(
  terminals: Array<{ terminalId: string }> = [],
): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    terminals,
  });
}

/** Attach the session over gRPC and wait for its terminal tab bar to render. */
function attachSession(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  tabs.tabs().should("exist");
}

// ---------------------------------------------------------------------------

describe("SessionTerminalTabs — Agent + bash terminals per session", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows a fixed Agent tab that is selected by default and has no close control", () => {
    // Given a connected session with no extra terminals
    const backend = aGrpcBackend();

    // When it is attached
    attachSession(backend);

    // Then the Agent tab is present, selected, and cannot be closed
    tabs.agentTab().should("exist").and("have.attr", "aria-selected", "true");
    tabs.tabClose("main").should("not.exist");
    tabs.pane("main").should("exist");
  });

  it("opens a new bash terminal via '+', focuses its tab, and streams its terminal_id", () => {
    // Given a connected session
    const backend = aGrpcBackend();
    attachSession(backend);

    // When the user opens a new terminal
    tabs.newTab().click();

    // Then StartTerminalSession was called for this session ...
    cy.wrap(backend).should((b: ConnectionServiceBackend) => {
      expect(b.startTerminalSessionIds).to.include(SESSION.sessionId);
    });

    // ... a new bash tab appears and becomes the selected tab ...
    tabs.tab("bash-1").should("exist").and("have.attr", "aria-selected", "true");
    tabs.agentTab().should("have.attr", "aria-selected", "false");

    // ... and its terminal opens an output stream addressed to the new terminal_id.
    cy.wrap(backend).should((b: ConnectionServiceBackend) => {
      expect(b.streamedTerminals.map((s) => s.terminalId)).to.include("bash-1");
    });
  });

  it("supports multiple terminals — a second '+' yields two bash tabs, both kept mounted", () => {
    // Given a connected session
    const backend = aGrpcBackend();
    attachSession(backend);

    // When two terminals are opened
    tabs.newTab().click();
    tabs.tab("bash-1").should("exist");
    tabs.newTab().click();

    // Then both bash tabs exist, the second is selected, and both terminals stay mounted
    // (backgrounding a tab must not unmount its terminal).
    tabs.tab("bash-2").should("exist").and("have.attr", "aria-selected", "true");
    tabs.tab("bash-1").should("exist").and("have.attr", "aria-selected", "false");
    tabs.pane("main").should("exist");
    tabs.pane("bash-1").should("exist");
    tabs.pane("bash-2").should("exist");
  });

  it("closing a bash tab stops that terminal and returns focus to the Agent tab", () => {
    // Given a connected session with one open bash terminal
    const backend = aGrpcBackend([{ terminalId: "bash-1" }]);
    attachSession(backend);
    tabs.tab("bash-1").click().should("have.attr", "aria-selected", "true");

    // When the user closes it
    tabs.tabClose("bash-1").click();

    // Then StopTerminalSession was called for that terminal_id ...
    cy.wrap(backend).should((b: ConnectionServiceBackend) => {
      expect(b.stoppedTerminals).to.deep.include({
        sessionId: SESSION.sessionId,
        terminalId: "bash-1",
      });
    });

    // ... the tab is gone and its terminal is torn down ...
    tabs.tab("bash-1").should("not.exist");
    tabs.pane("bash-1").should("not.exist");

    // ... and focus falls back to the Agent tab.
    tabs.agentTab().should("have.attr", "aria-selected", "true");
  });

  it("routes keyboard input to the active terminal's terminal_id", () => {
    // Given a connected session with the Agent tab active
    const backend = aGrpcBackend();
    attachSession(backend);

    // When a new bash terminal is opened (becomes active) and the user types into it
    tabs.newTab().click();
    tabs.pane("bash-1").should("exist");
    tabs.paneTerminal("bash-1").click().type("ls\n");

    // Then the input is sent tagged with the active terminal's id, not "main".
    cy.wrap(backend).should((b: ConnectionServiceBackend) => {
      const targets = new Set(b.sentTerminalInput.map((i) => i.terminalId));
      expect(targets).to.include("bash-1");
      expect(b.sentTerminalInput.every((i) => i.sessionId === SESSION.sessionId)).to.equal(true);
    });
  });
});

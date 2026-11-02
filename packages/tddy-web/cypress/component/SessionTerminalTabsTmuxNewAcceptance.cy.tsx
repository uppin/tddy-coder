/**
 * Cypress component acceptance: start a new tmux session as a session tab.
 *
 * "+" → "New tmux session…" opens a name field prefilled with a name no listed tmux session has;
 * Enter starts a `tmux-new` terminal and focuses its `tmux: <name>` tab, Escape cancels.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-new-session-tab.md (`#session-tabs 7/7`)
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

/** The session's short id — the first segment of its id — is what a new tmux session is named. */
const SHORT_ID = "c0ffee12";

const SESSION = {
  sessionId: `${SHORT_ID}-0000-0000-0000-000000000001`,
  createdAt: "2026-11-02T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-tmux-new-1",
  daemonInstanceId: "local",
  sessionType: "claude-cli",
  pendingElicitation: false,
};

function aBackendWithTmuxSessions(names: string[]): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    newTerminalId: (index) => `tmux-tab-${index + 1}`,
    tmuxSessions: names.map((name) => ({ name })),
  });
}

function openNewTmuxSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  tabs.newTab().click();
  tabs.tmuxNewOption().click();
}

describe("A new tmux session as a session tab", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("New tmux session… prefills a name not among the listed sessions", () => {
    // Given a host already running a tmux session named after this session
    const backend = aBackendWithTmuxSessions([SHORT_ID]);

    // When
    openNewTmuxSession(backend);

    // Then
    tabs.tmuxNameInput().should("have.value", `${SHORT_ID}-2`);
  });

  it('Enter opens a "tmux: <name>" tab for the typed name', () => {
    // Given
    const backend = aBackendWithTmuxSessions([]);
    openNewTmuxSession(backend);

    // When
    tabs.tmuxNameInput().clear().type("scratch{enter}");

    // Then
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startedTerminalKinds).to.deep.equal(["tmux-new"]);
      expect(b.startedTmuxSessions).to.deep.equal(["scratch"]);
    });
    tabs.tab("tmux-tab-1").should("have.attr", "aria-selected", "true").and("contain.text", "tmux: scratch");
  });

  it("Escape cancels without starting a terminal", () => {
    // Given
    const backend = aBackendWithTmuxSessions([]);
    openNewTmuxSession(backend);

    // When
    tabs.tmuxNameInput().type("{esc}");

    // Then
    tabs.tmuxNameInput().should("not.exist");
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startTerminalSessionIds).to.deep.equal([]);
    });
  });
});

/**
 * Cypress component acceptance: attach a running tmux session as a session tab.
 *
 * The "+" menu lists the tmux sessions on the session's host (`ListTmuxSessions`); choosing one
 * starts a `tmux` terminal attached to it, labelled `tmux: <name>`.
 *
 * Feature: docs/ft/web/session-terminal-tabs.md (`#session-tabs 3/7`)
 *
 * Driven over the deterministic gRPC path (empty `livekitRoom`), so every terminal RPC flows over
 * the daemon client into the in-memory backend.
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

const SESSION = {
  sessionId: "tmux-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-11-02T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-tmux-tabs-1",
  daemonInstanceId: "local",
  sessionType: "claude-cli",
  pendingElicitation: false,
};

function aBackendWithTmux(
  scenario: Pick<Parameters<typeof aSessionServiceBackend>[0] & object, "tmuxSessions" | "tmuxUnavailable">,
): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    newTerminalId: (index) => `tmux-tab-${index + 1}`,
    ...scenario,
  });
}

function attachSessionAndOpenTheMenu(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  tabs.newTab().click();
}

describe("A running tmux session as a session tab", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it('"+" lists the host\'s tmux sessions', () => {
    // Given a host running two tmux sessions
    const backend = aBackendWithTmux({
      tmuxSessions: [{ name: "alpha", windows: 2 }, { name: "beta" }],
    });

    // When the "+" menu is opened
    attachSessionAndOpenTheMenu(backend);

    // Then both are offered
    tabs.tmuxOption("alpha").should("be.visible");
    tabs.tmuxOption("beta").should("be.visible");
  });

  it('choosing a tmux session opens a tab labelled "tmux: <name>"', () => {
    // Given
    const backend = aBackendWithTmux({ tmuxSessions: [{ name: "alpha" }] });
    attachSessionAndOpenTheMenu(backend);

    // When
    tabs.tmuxOption("alpha").click();

    // Then a tmux terminal attached to alpha was started, and its tab is selected and named
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(b.startedTerminalKinds).to.deep.equal(["tmux"]);
      expect(b.startedTmuxSessions).to.deep.equal(["alpha"]);
    });
    tabs
      .tab("tmux-tab-1")
      .should("have.attr", "aria-selected", "true")
      .and("contain.text", "tmux: alpha");
  });

  it('"+" shows "No tmux sessions" when there are none', () => {
    // Given a host with no tmux sessions
    const backend = aBackendWithTmux({ tmuxSessions: [] });

    // When
    attachSessionAndOpenTheMenu(backend);

    // Then
    tabs.tmuxEmpty().should("be.visible").and("contain.text", "No tmux sessions");
  });

  it('"+" shows the error when tmux is unavailable', () => {
    // Given a host without tmux
    const backend = aBackendWithTmux({ tmuxUnavailable: "tmux is not installed on this host" });

    // When
    attachSessionAndOpenTheMenu(backend);

    // Then
    tabs.tmuxError().should("be.visible").and("contain.text", "tmux is not installed on this host");
  });
});

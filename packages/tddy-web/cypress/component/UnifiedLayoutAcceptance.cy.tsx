/**
 * Acceptance tests: the unified AppShell puts the hamburger navigation menu on every screen,
 * the menu offers a single "Sessions" entry (no legacy "Sessions (new)" duplicate), and a
 * dedicated "LiveKit" entry.
 *
 * PRD: docs/ft/web/app-shell.md
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { DaemonNavMenu } from "../../src/components/shell/DaemonNavMenu";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { appShellPage as shell } from "../support/pages/appShellPage";

const A_SESSION = {
  sessionId: "unify-aaaaaaaa-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T12:00:00Z",
  status: "exited",
  repoPath: "/home/dev/feature",
  pid: 0,
  isActive: false,
  projectId: "proj-unify-1",
  workflowGoal: "A session",
  pendingElicitation: false,
};

describe("Unified layout — navigation menu on every screen", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows the top-left navigation menu on the sessions drawer screen", () => {
    // Given — the sessions drawer with one session
    const backend = aConnectionServiceBackend({ sessions: [A_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen onNavigate={cy.stub()} />),
      backend,
    );

    // Then — the shared hamburger menu button is present
    shell.menuButton().should("be.visible");
  });
});

describe("Unified layout — navigation menu contents", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("offers a single Sessions entry and no legacy 'Sessions (new)' duplicate", () => {
    // Given / When — the navigation menu is opened
    cy.mount(<DaemonNavMenu onNavigate={cy.stub()} />);
    shell.openMenu();

    // Then — exactly one "Sessions" entry, and no "Sessions (new)"
    shell.sessionsItem().should("have.text", "Sessions");
    shell.menu().should("not.contain.text", "Sessions (new)");
  });

  it("navigates to the LiveKit route when the LiveKit entry is chosen", () => {
    // Given — a navigation menu with a spied onNavigate
    const onNavigate = cy.stub().as("onNavigate");
    cy.mount(<DaemonNavMenu onNavigate={onNavigate} />);

    // When — open the menu and choose LiveKit
    shell.openMenu();
    shell.livekitItem().click();

    // Then — routed to /livekit
    cy.get("@onNavigate").should("have.been.calledOnceWith", "/livekit");
  });
});

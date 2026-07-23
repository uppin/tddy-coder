/**
 * Unit/component tests for the shared AppShell: it renders the unified header (hamburger menu,
 * title, daemon selector, user avatar) and the screen body, and forwards menu navigation.
 *
 * PRD: docs/ft/web/app-shell.md
 */

import React from "react";
import { AppShell } from "../../src/components/shell/AppShell";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { appShellPage as shell } from "../support/pages/appShellPage";
import { byTestId, TEST_IDS } from "../support/testIds";

describe("AppShell — unified header", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("renders the hamburger menu, title, daemon selector, and body", () => {
    // Given / When — a scroll-variant shell around a body
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(
        <AppShell title="Projects" onNavigate={cy.stub()} variant="scroll">
          <div data-testid="shell-body">body content</div>
        </AppShell>,
      ),
      aConnectionServiceBackend(),
    );

    // Then — header chrome and body are present
    shell.menuButton().should("be.visible");
    cy.contains("Projects").should("be.visible");
    byTestId(TEST_IDS.daemonSelectorTrigger).should("exist");
    byTestId("shell-body").should("have.text", "body content");
  });

  it("forwards the chosen destination through onNavigate", () => {
    // Given — a shell with a spied onNavigate
    const onNavigate = cy.stub().as("onNavigate");
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(
        <AppShell title="Projects" onNavigate={onNavigate} variant="scroll">
          <div data-testid="shell-body">body content</div>
        </AppShell>,
      ),
      aConnectionServiceBackend(),
    );

    // When — open the menu and choose Sessions
    shell.openMenu();
    shell.sessionsItem().click();

    // Then
    cy.get("@onNavigate").should("have.been.calledOnceWith", "/sessions");
  });

  it("renders the body in the fullbleed variant", () => {
    // Given / When — a fullbleed-variant shell (drawer screens use this)
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(
        <AppShell
          title="Sessions"
          onNavigate={cy.stub()}
          variant="fullbleed"
          dataTestId="app-shell-fullbleed"
        >
          <div data-testid="shell-body">body content</div>
        </AppShell>,
      ),
      aConnectionServiceBackend(),
    );

    // Then — the fullbleed root and its body render, with the header still present
    byTestId("app-shell-fullbleed").should("exist");
    shell.menuButton().should("be.visible");
    byTestId("shell-body").should("have.text", "body content");
  });
});

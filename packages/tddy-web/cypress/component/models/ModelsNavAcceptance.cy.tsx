/**
 * Acceptance: the app shell's hamburger menu offers a **Models & Agents** entry that navigates to
 * `#/models`.
 *
 * The shell owns the menu for every daemon-mode screen (`AppShell` → `DaemonNavMenu`), so the entry
 * is asserted there rather than on the Models screen itself.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC1).
 */

import React from "react";
import { AppShell } from "../../../src/components/shell/AppShell";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../../support/rpc/recordingLiveKitRpc";
import { appShellPage as shell } from "../../support/pages/appShellPage";

describe("ModelsNavAcceptance — Models & Agents in the navigation menu", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    // Seed inside `cy.then` so it runs *after* the queued clears above; a bare synchronous
    // `setItem` executes first and is then wiped, leaving the screen with no session token.
    cy.then(() => window.localStorage.setItem("tddy_session_token", "fake-token"));
  });

  it("navigates to the models screen when Models & Agents is chosen from the menu", () => {
    // Given — a shell with a spied navigation handler
    const onNavigate = cy.stub().as("onNavigate");
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(
        <AppShell title="Sessions" onNavigate={onNavigate} variant="scroll">
          <div data-testid="shell-body">body content</div>
        </AppShell>,
      ),
      aConnectionServiceBackend(),
    );

    // When — open the menu and choose Models & Agents
    shell.openMenu();
    shell.modelsItem().click();

    // Then
    cy.get("@onNavigate").should("have.been.calledOnceWith", "/models");
  });

  it("lists Models & Agents directly after Projects in the menu", () => {
    // Given
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(
        <AppShell title="Sessions" onNavigate={cy.stub()} variant="scroll">
          <div data-testid="shell-body">body content</div>
        </AppShell>,
      ),
      aConnectionServiceBackend(),
    );

    // When
    shell.openMenu();

    // Then — the full menu, in order, so a misplaced or duplicated entry fails the test
    shell
      .menuItemLabels()
      .should("deep.equal", [
        "Sessions",
        "Worktrees",
        "Tasks",
        "Projects",
        "Models & Agents",
        "VMs",
        "LiveKit",
        "RPC Playground",
      ]);
  });
});

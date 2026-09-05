/**
 * Acceptance tests for the `#/settings` route.
 *
 * The daemon settings screen existed but nothing reached it: its only importer was a test driver.
 * The route puts it behind the same shell, the same navigation menu and the same sign-in gate every
 * other daemon-mode screen uses, pointed at the daemon serving this page — the one whose YAML the
 * operator is editing.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` (M9).
 */

import React from "react";
import { AppShell } from "../../src/components/shell/AppShell";
import { appShellPage as shell } from "../support/pages/appShellPage";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  A_SIGNED_IN_SESSION_TOKEN,
  aServingDaemonWithASignedInOperator,
  theApp,
} from "../support/drivers/settingsRouteDriver";

describe("The settings route", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("shows the serving daemon's configuration at #/settings", () => {
    // Given a daemon serving the page, with the operator signed in
    const app = theApp(aServingDaemonWithASignedInOperator());

    // When the operator opens the settings route
    app.openedAt("#/settings");

    // Then that daemon's own settings are on screen
    app.expectLiveKitUrl("ws://127.0.0.1:7880");
  });

  it("reads the configuration with the signed-in operator's session token", () => {
    // Given a daemon serving the page, with the operator signed in
    const app = theApp(aServingDaemonWithASignedInOperator());

    // When the operator opens the settings route
    app.openedAt("#/settings");

    // Then the read was gated by the token that operator holds, not by an empty one
    app.expectConfigurationReadWith(A_SIGNED_IN_SESSION_TOKEN);
  });
});

describe("The daemon-mode navigation menu", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    // Seed inside `cy.then` so it runs *after* the queued clears above.
    cy.then(() => window.localStorage.setItem("tddy_session_token", "fake-token"));
  });

  it("navigates to the settings route when Settings is chosen", () => {
    // Given the shell every daemon-mode screen carries, with a spied navigation handler
    const onNavigate = cy.stub().as("onNavigate");
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(
        <AppShell title="Sessions" onNavigate={onNavigate} variant="scroll">
          <div data-testid="shell-body">body content</div>
        </AppShell>,
      ),
      aConnectionServiceBackend(),
    );

    // When the operator opens the menu and chooses Settings
    shell.openMenu();
    shell.settingsItem().click();

    // Then
    cy.get("@onNavigate").should("have.been.calledOnceWith", "/settings");
  });
});

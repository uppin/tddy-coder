/**
 * Acceptance tests: VNC tab in the session inspector drawer.
 *
 * PRD: docs/ft/web/vnc-sessions.md (AC-VNC-1, AC-VNC-2, AC-VNC-3).
 *
 * Tests the VNC tab integration via the full `SessionsDrawerScreen`, using
 * intercepted RPCs for both ConnectionService and VncService.
 *
 * These tests are intentionally failing until:
 *   1. `InspectorTabs.tsx` adds `"vnc"` to `InspectorTab` union and renders the tab button.
 *   2. `SessionInspectorDrawer.tsx` renders `SessionVncTab` when the VNC tab is active.
 *   3. `SessionVncTab.tsx` shows the target list and Add form.
 *   4. `VncPassphraseDialog.tsx` is shown when an operation requires the vault.
 *   5. `VncService` RPC helpers are complete (`buf generate` has run).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  interceptConnectionRpcs,
  interceptConnectSession,
} from "../support/rpc/connectionRpcs";
import {
  interceptListVncTargets,
  interceptAddVncTarget,
  interceptUnlockVncVault,
} from "../support/rpc/vncRpcs";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ACTIVE_SESSION = {
  sessionId: "vnc-test-session-aabbccdd-0000-0000-0000-000000000001",
  createdAt: "2026-06-26T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/vnc-project",
  pid: 12345,
  isActive: false,  // disconnected so inspector auto-opens
  projectId: "proj-vnc-1",
  daemonInstanceId: "",
  workflowGoal: "VNC test session",
  pendingElicitation: false,
};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// AC-VNC-1: VNC tab is present in the inspector tab strip
// ---------------------------------------------------------------------------

it("shows a VNC tab in the inspector tab strip alongside Details and Tools", () => {
  // Given
  interceptConnectionRpcs([ACTIVE_SESSION]);
  interceptListVncTargets([]);

  // When
  cy.mount(<SessionsDrawerScreen />);
  cy.wait("@listSessions");
  page.drawerItem(ACTIVE_SESSION.sessionId).click();
  page.inspectorDrawer().should("have.attr", "data-state", "open");

  // Then — all three tabs are present
  cy.get(`[data-testid="sessions-inspector-tab-details"]`).should("exist");
  cy.get(`[data-testid="sessions-inspector-tab-tools"]`).should("exist");
  cy.get(`[data-testid="${"sessions-inspector-tab-vnc"}"]`).should("exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-1: Switching to the VNC tab renders the VNC panel
// ---------------------------------------------------------------------------

it("renders the VNC tab panel and hides the metadata panel when the VNC tab is clicked", () => {
  // Given
  interceptConnectionRpcs([ACTIVE_SESSION]);
  interceptListVncTargets([]);

  // When
  cy.mount(<SessionsDrawerScreen />);
  cy.wait("@listSessions");
  page.drawerItem(ACTIVE_SESSION.sessionId).click();
  page.inspectorDrawer().should("have.attr", "data-state", "open");

  // Switch to VNC tab
  page.inspectorVncTab().click();

  // Then — VNC panel visible, metadata hidden
  page.vncTabPanel().should("exist");
  page.inspectorMetadata().should("not.exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-2: Empty state — no targets, Add form present
// ---------------------------------------------------------------------------

it("shows an empty target list and an Add form when no VNC targets are configured", () => {
  // Given — no targets
  interceptConnectionRpcs([ACTIVE_SESSION]);
  interceptListVncTargets([]);

  // When
  cy.mount(<SessionsDrawerScreen />);
  cy.wait("@listSessions");
  page.drawerItem(ACTIVE_SESSION.sessionId).click();
  page.inspectorVncTab().click();

  // Then
  page.vncTabPanel().should("exist");
  page.vncTargetList().should("exist");
  page.vncTargetList().should("not.contain.text", "row"); // no target rows
  page.vncAddForm().should("exist");
  page.vncAddSubmit().should("exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-3: Submitting the Add form calls AddVncTarget (passphrase-less target)
// ---------------------------------------------------------------------------

it("calls AddVncTarget RPC when the Add form is submitted with a password-less target", () => {
  // Given — vault is unlocked (no password in this scenario)
  interceptConnectionRpcs([ACTIVE_SESSION]);
  interceptListVncTargets([]);
  interceptAddVncTarget({ id: "t-001", label: "Dev VM", host: "192.168.1.5", port: 5900 });

  // When
  cy.mount(<SessionsDrawerScreen />);
  cy.wait("@listSessions");
  page.drawerItem(ACTIVE_SESSION.sessionId).click();
  page.inspectorVncTab().click();

  // Fill and submit the Add form (no password = vault not needed)
  page.vncAddLabel().type("Dev VM");
  page.vncAddHost().type("192.168.1.5");
  page.vncAddPort().clear().type("5900");
  page.vncAddSubmit().click();

  // Then — AddVncTarget RPC was called
  cy.wait("@addVncTarget");
});

// ---------------------------------------------------------------------------
// AC-VNC-3: Submitting with a password shows the passphrase dialog first
// ---------------------------------------------------------------------------

it("shows the passphrase dialog before adding a target that has a password", () => {
  // Given
  interceptConnectionRpcs([ACTIVE_SESSION]);
  interceptListVncTargets([]);
  interceptUnlockVncVault(true);
  interceptAddVncTarget({ id: "t-002", label: "Secure VM", host: "10.0.0.1", port: 5900 });

  // When
  cy.mount(<SessionsDrawerScreen />);
  cy.wait("@listSessions");
  page.drawerItem(ACTIVE_SESSION.sessionId).click();
  page.inspectorVncTab().click();

  // Fill Add form with a password
  page.vncAddLabel().type("Secure VM");
  page.vncAddHost().type("10.0.0.1");
  page.vncAddPort().clear().type("5900");
  page.vncAddPassword().type("s3cr3t");
  page.vncAddSubmit().click();

  // Then — passphrase dialog appears before the add completes
  page.vncPassphraseDialog().should("exist").and("be.visible");

  // When — user enters passphrase and confirms
  page.vncPassphraseInput().type("my-vault-passphrase");
  page.vncPassphraseConfirm().click();

  // Then — UnlockVncVault and AddVncTarget RPCs were called in sequence
  cy.wait("@unlockVncVault");
  cy.wait("@addVncTarget");
});

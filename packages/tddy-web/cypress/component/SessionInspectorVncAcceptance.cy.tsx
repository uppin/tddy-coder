/**
 * Acceptance tests: VNC tab in the session inspector drawer.
 *
 * PRD: docs/ft/web/vnc-sessions.md (AC-VNC-1, AC-VNC-2, AC-VNC-3).
 *
 * Exercises the VNC tab integration via the full SessionsDrawerScreen.
 * All RPC calls are handled by an in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { VncService } from "../../src/gen/vnc_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "vnc-test-session-aabbccdd-0000-0000-0000-000000000001",
  createdAt: "2026-06-26T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/vnc-project",
  pid: 12345,
  isActive: false,
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
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorDrawer().should("have.attr", "data-state", "open");

  // Then
  page.inspectorVncTab().should("exist");
  cy.get(`[data-testid="sessions-inspector-tab-details"]`).should("exist");
  cy.get(`[data-testid="sessions-inspector-tab-tools"]`).should("exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-1: Switching to the VNC tab renders the VNC panel
// ---------------------------------------------------------------------------

it("renders the VNC tab panel and hides the metadata panel when the VNC tab is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorVncTab().click();

  // Then
  page.vncTabPanel().should("exist");
  page.inspectorMetadata().should("not.exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-2: Empty state — no targets, Add form present
// ---------------------------------------------------------------------------

it("shows an empty target list and an Add form when no VNC targets are configured", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorVncTab().click();

  // Then
  page.vncTabPanel().should("exist");
  page.vncTargetList().should("exist");
  page.vncAddForm().should("exist");
  page.vncAddSubmit().should("exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-3: Submitting the Add form calls AddVncTarget (passphrase-less target)
// ---------------------------------------------------------------------------

it("calls AddVncTarget with correct fields when the form is submitted without a password", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }))
    .onUnary(VncService.method.addVncTarget, (req) => ({
      target: { id: "t-001", label: req.label, host: req.host, port: req.port },
    }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorVncTab().click();
  page.vncAddLabel().type("Dev VM");
  page.vncAddHost().type("192.168.1.5");
  page.vncAddPort().clear().type("5900");
  page.vncAddSubmit().click();

  // Then — AddVncTarget received the right fields
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(VncService.method.addVncTarget);
    expect(calls).to.have.length(1);
    expect(calls[0].label).to.equal("Dev VM");
    expect(calls[0].host).to.equal("192.168.1.5");
    expect(calls[0].port).to.equal(5900);
    expect(calls[0].password).to.equal("");
  });
});

// ---------------------------------------------------------------------------
// AC-VNC-3: Submitting with a password shows the passphrase dialog first
// ---------------------------------------------------------------------------

it("shows the passphrase dialog before adding a target that has a password", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }))
    .onUnary(VncService.method.unlockVncVault, () => ({ ok: true }))
    .onUnary(VncService.method.addVncTarget, (req) => ({
      target: { id: "t-002", label: req.label, host: req.host, port: req.port },
    }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorVncTab().click();
  page.vncAddLabel().type("Secure VM");
  page.vncAddHost().type("10.0.0.1");
  page.vncAddPort().clear().type("5900");
  page.vncAddPassword().type("s3cr3t");
  page.vncAddSubmit().click();

  // Then — passphrase dialog appears
  page.vncPassphraseDialog().should("exist").and("be.visible");

  // When — user confirms passphrase
  page.vncPassphraseInput().type("my-vault-passphrase");
  page.vncPassphraseConfirm().click();

  // Then — UnlockVncVault was called before AddVncTarget
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(VncService.method.unlockVncVault)).to.have.length(1);
    expect(b.callsTo(VncService.method.addVncTarget)).to.have.length(1);
  });
});

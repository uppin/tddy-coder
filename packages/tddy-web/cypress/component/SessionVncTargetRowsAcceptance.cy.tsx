/**
 * Acceptance tests: VNC target rows in the session inspector drawer.
 *
 * PRD: docs/ft/web/vnc-sessions.md (AC-VNC-3, AC-VNC-4, AC-VNC-5, AC-VNC-6).
 *
 * Tests per-target row UI (Start / Stop / Remove buttons) and the VncOverlay
 * lifecycle. All RPC calls flow through the in-memory backend — no HTTP intercepts.
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
  sessionId: "vnc-row-session-1111-0000-0000-0000-000000000002",
  createdAt: "2026-06-26T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/row-project",
  pid: 22222,
  isActive: false,
  projectId: "proj-vnc-rows",
  daemonInstanceId: "",
  workflowGoal: "Row test session",
  pendingElicitation: false,
};

const TARGET = { id: "t-row-001", label: "Dev Box", host: "192.168.10.5", port: 5900 };

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Shared helper — open the inspector to the VNC tab
// ---------------------------------------------------------------------------

function openVncTab(backend: ReturnType<typeof aSessionsDrawerBackend>) {
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorVncTab().click();
}

// ---------------------------------------------------------------------------
// AC-VNC-3: Target rows visible when ListVncTargets returns targets
// ---------------------------------------------------------------------------

it("shows a target row with Start and Remove buttons for each configured VNC target", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [TARGET] }));

  // When
  openVncTab(backend);

  // Then
  page.vncTargetRow(TARGET.id).should("exist");
  page.vncStartBtn(TARGET.id).should("exist");
  page.vncRemoveBtn(TARGET.id).should("exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-4: Start button calls StartVncStream with the correct target id
// ---------------------------------------------------------------------------

it("calls StartVncStream with the target id when the Start button is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [TARGET] }))
    .onUnary(VncService.method.startVncStream, () => ({
      livekitRoom: "room-vnc-test",
      livekitUrl: "ws://127.0.0.1:7880",
      bridgeIdentity: `vnc-bridge-${TARGET.id}`,
      trackName: `vnc:${TARGET.id}`,
      width: 1920,
      height: 1080,
    }));

  // When
  openVncTab(backend);
  page.vncStartBtn(TARGET.id).click();

  // Then — StartVncStream called with correct target id
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(VncService.method.startVncStream);
    expect(calls).to.have.length(1);
    expect(calls[0].targetId).to.equal(TARGET.id);
  });
});

// ---------------------------------------------------------------------------
// AC-VNC-5: VncOverlay appears after StartVncStream returns bridge coordinates
// ---------------------------------------------------------------------------

it("shows the VncOverlay after StartVncStream returns bridge coordinates", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [TARGET] }))
    .onUnary(VncService.method.startVncStream, () => ({
      livekitRoom: "room-vnc-row-test",
      livekitUrl: "ws://127.0.0.1:7880",
      bridgeIdentity: `vnc-bridge-${TARGET.id}`,
      trackName: `vnc:${TARGET.id}`,
      width: 1280,
      height: 800,
    }));

  // When
  openVncTab(backend);
  page.vncStartBtn(TARGET.id).click();

  // Then — overlay is visible with the expected elements
  page.vncOverlay().should("exist").and("be.visible");
  page.vncOverlayClose().should("exist");
  page.vncOverlayVideo().should("exist");
});

// ---------------------------------------------------------------------------
// AC-VNC-6: Closing the overlay calls StopVncStream with the correct target id
// ---------------------------------------------------------------------------

it("calls StopVncStream with the target id when the overlay close button is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [TARGET] }))
    .onUnary(VncService.method.startVncStream, () => ({
      livekitRoom: "room-vnc-stop-test",
      livekitUrl: "ws://127.0.0.1:7880",
      bridgeIdentity: `vnc-bridge-${TARGET.id}`,
      trackName: `vnc:${TARGET.id}`,
      width: 1920,
      height: 1080,
    }))
    .onUnary(VncService.method.stopVncStream, () => ({ ok: true }));

  // When — start then close
  openVncTab(backend);
  page.vncStartBtn(TARGET.id).click();
  page.vncOverlay().should("exist");
  page.vncOverlayClose().click();

  // Then — overlay gone and StopVncStream called with the right target
  page.vncOverlay().should("not.exist");
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(VncService.method.stopVncStream);
    expect(calls).to.have.length(1);
    expect(calls[0].targetId).to.equal(TARGET.id);
  });
});

// ---------------------------------------------------------------------------
// AC-VNC-3: Remove button calls RemoveVncTarget and removes the row
// ---------------------------------------------------------------------------

it("calls RemoveVncTarget with the target id and removes the row from the list", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(VncService.method.listVncTargets, () => ({ targets: [TARGET] }))
    .onUnary(VncService.method.removeVncTarget, () => ({ ok: true }));

  // When
  openVncTab(backend);
  page.vncRemoveBtn(TARGET.id).click();

  // Then — RemoveVncTarget called with the right id and row is gone
  page.vncTargetRow(TARGET.id).should("not.exist");
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(VncService.method.removeVncTarget);
    expect(calls).to.have.length(1);
    expect(calls[0].targetId).to.equal(TARGET.id);
  });
});

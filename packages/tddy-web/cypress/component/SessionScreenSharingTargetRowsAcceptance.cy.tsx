/**
 * Acceptance tests: Screen Sharing target rows in the session inspector drawer.
 *
 * PRD: docs/ft/web/screen-sharing-sessions.md (AC-SS-2, AC-SS-5, AC-SS-6, AC-SS-7).
 *
 * Tests per-target row UI (Start / Stop / Remove buttons) and the
 * ScreenSharingOverlay lifecycle. All RPC calls flow through the in-memory
 * backend — no HTTP intercepts.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { Protocol, ScreenSharingService } from "../../src/gen/screen_sharing_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/screenSharingBackend";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "ss-row-session-1111-0000-0000-0000-000000000002",
  createdAt: "2026-06-26T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/row-project",
  pid: 22222,
  isActive: false,
  projectId: "proj-ss-rows",
  daemonInstanceId: "",
  workflowGoal: "Row test session",
  pendingElicitation: false,
};

const TARGET = {
  id: "t-row-001",
  label: "Dev Box",
  host: "192.168.10.5",
  port: 5900,
  protocol: Protocol.VNC,
  username: "",
};

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
// Shared helper — open the inspector to the Screen Sharing tab
// ---------------------------------------------------------------------------

function openScreenSharingTab(backend: ReturnType<typeof aSessionsDrawerBackend>) {
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();
}

// ---------------------------------------------------------------------------
// AC-SS-2: Target rows visible when ListTargets returns targets
// ---------------------------------------------------------------------------

it("shows a target row with protocol label, Start and Remove buttons for each configured target", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [TARGET] }));

  // When
  openScreenSharingTab(backend);

  // Then
  page.screenSharingTargetRow(TARGET.id).should("exist");
  page.screenSharingStartBtn(TARGET.id).should("exist");
  page.screenSharingRemoveBtn(TARGET.id).should("exist");
  // The protocol label is visible in the target row
  page.screenSharingTargetRow(TARGET.id).should("contain.text", "VNC");
});

// ---------------------------------------------------------------------------
// AC-SS-5: Start button calls StartStream with the correct target id
// ---------------------------------------------------------------------------

it("calls StartStream with the target id when the Start button is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [TARGET] }))
    .onUnary(ScreenSharingService.method.startStream, () => ({
      livekitRoom: "room-ss-test",
      livekitUrl: "ws://127.0.0.1:7880",
      bridgeIdentity: `screenshare-bridge-${TARGET.id}`,
      trackName: `screenshare:${TARGET.id}`,
      width: 1920,
      height: 1080,
    }));

  // When
  openScreenSharingTab(backend);
  page.screenSharingStartBtn(TARGET.id).click();

  // Then — StartStream called with correct target id
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ScreenSharingService.method.startStream);
    expect(calls).to.have.length(1);
    expect(calls[0].targetId).to.equal(TARGET.id);
  });
});

// ---------------------------------------------------------------------------
// AC-SS-5: ScreenSharingOverlay appears after StartStream returns coordinates
// ---------------------------------------------------------------------------

it("shows the ScreenSharingOverlay after StartStream returns bridge coordinates", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [TARGET] }))
    .onUnary(ScreenSharingService.method.startStream, () => ({
      livekitRoom: "room-ss-row-test",
      livekitUrl: "ws://127.0.0.1:7880",
      bridgeIdentity: `screenshare-bridge-${TARGET.id}`,
      trackName: `screenshare:${TARGET.id}`,
      width: 1280,
      height: 800,
    }));

  // When
  openScreenSharingTab(backend);
  page.screenSharingStartBtn(TARGET.id).click();

  // Then — overlay is visible with expected elements
  page.screenSharingOverlay().should("exist").and("be.visible");
  page.screenSharingOverlayClose().should("exist");
  page.screenSharingOverlayVideo().should("exist");
});

// ---------------------------------------------------------------------------
// AC-SS-7: Closing the overlay calls StopStream with the correct target id
// ---------------------------------------------------------------------------

it("calls StopStream with the target id when the overlay close button is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [TARGET] }))
    .onUnary(ScreenSharingService.method.startStream, () => ({
      livekitRoom: "room-ss-stop-test",
      livekitUrl: "ws://127.0.0.1:7880",
      bridgeIdentity: `screenshare-bridge-${TARGET.id}`,
      trackName: `screenshare:${TARGET.id}`,
      width: 1920,
      height: 1080,
    }))
    .onUnary(ScreenSharingService.method.stopStream, () => ({ ok: true }));

  // When — start then close
  openScreenSharingTab(backend);
  page.screenSharingStartBtn(TARGET.id).click();
  page.screenSharingOverlay().should("exist");
  page.screenSharingOverlayClose().click();

  // Then — overlay gone and StopStream called with the right target
  page.screenSharingOverlay().should("not.exist");
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ScreenSharingService.method.stopStream);
    expect(calls).to.have.length(1);
    expect(calls[0].targetId).to.equal(TARGET.id);
  });
});

// ---------------------------------------------------------------------------
// AC-SS-2: Remove button calls RemoveTarget and removes the row
// ---------------------------------------------------------------------------

it("calls RemoveTarget with the target id and removes the row from the list", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [TARGET] }))
    .onUnary(ScreenSharingService.method.removeTarget, () => ({ ok: true }));

  // When
  openScreenSharingTab(backend);
  page.screenSharingRemoveBtn(TARGET.id).click();

  // Then — RemoveTarget called with the right id and row is gone
  page.screenSharingTargetRow(TARGET.id).should("not.exist");
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ScreenSharingService.method.removeTarget);
    expect(calls).to.have.length(1);
    expect(calls[0].targetId).to.equal(TARGET.id);
  });
});

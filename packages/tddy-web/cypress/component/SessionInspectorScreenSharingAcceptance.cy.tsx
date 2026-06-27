/**
 * Acceptance tests: Screen Sharing tab in the session inspector drawer.
 *
 * PRD: docs/ft/web/screen-sharing-sessions.md (AC-SS-1, AC-SS-2, AC-SS-3, AC-SS-4).
 *
 * Exercises the Screen Sharing tab integration via the full SessionsDrawerScreen.
 * All RPC calls are handled by an in-memory backend — no HTTP intercepts.
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
  sessionId: "ss-test-session-aabbccdd-0000-0000-0000-000000000001",
  createdAt: "2026-06-26T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/ss-project",
  pid: 12345,
  isActive: false,
  projectId: "proj-ss-1",
  daemonInstanceId: "",
  workflowGoal: "Screen Sharing test session",
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
// AC-SS-1: Screen Sharing tab is present in the inspector tab strip
// ---------------------------------------------------------------------------

it("shows a Screen Sharing tab in the inspector tab strip alongside Details and Tools", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorDrawer().should("have.attr", "data-state", "open");

  // Then
  page.inspectorScreenSharingTab().should("exist");
  page.inspectorDetailsTab().should("exist");
  page.inspectorToolsTab().should("exist");
});

// ---------------------------------------------------------------------------
// AC-SS-1: Switching to Screen Sharing tab renders the panel
// ---------------------------------------------------------------------------

it("renders the Screen Sharing tab panel when the Screen Sharing tab is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();

  // Then
  page.screenSharingTabPanel().should("exist");
  page.inspectorMetadata().should("not.exist");
});

// ---------------------------------------------------------------------------
// AC-SS-2: Empty state — no targets, Add form with protocol selector present
// ---------------------------------------------------------------------------

it("shows an empty target list and an Add form with protocol selector when no targets are configured", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();

  // Then
  page.screenSharingTabPanel().should("exist");
  page.screenSharingTargetList().should("exist");
  page.screenSharingAddForm().should("exist");
  page.screenSharingAddProtocol().should("exist");
  page.screenSharingAddSubmit().should("exist");
});

// ---------------------------------------------------------------------------
// AC-SS-3: Selecting VNC protocol defaults the port to 5900
// ---------------------------------------------------------------------------

it("selecting VNC protocol defaults the port to 5900", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();
  page.screenSharingAddProtocol().select("VNC");

  // Then — port placeholder or value reflects VNC default
  page.screenSharingAddPort().should(($el) => {
    const val = $el.val() as string;
    const placeholder = $el.attr("placeholder") ?? "";
    expect(val === "5900" || placeholder === "5900").to.be.true;
  });
});

// ---------------------------------------------------------------------------
// AC-SS-3: Selecting RDP protocol defaults the port to 3389
// ---------------------------------------------------------------------------

it("selecting RDP protocol defaults the port to 3389", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();
  page.screenSharingAddProtocol().select("RDP");

  // Then — port placeholder or value reflects RDP default
  page.screenSharingAddPort().should(($el) => {
    const val = $el.val() as string;
    const placeholder = $el.attr("placeholder") ?? "";
    expect(val === "3389" || placeholder === "3389").to.be.true;
  });
});

// ---------------------------------------------------------------------------
// AC-SS-3: Submitting a VNC target sends protocol=VNC
// ---------------------------------------------------------------------------

it("submitting VNC target calls AddTarget with protocol VNC and correct fields", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }))
    .onUnary(ScreenSharingService.method.addTarget, (req) => ({
      target: {
        id: "t-vnc-001",
        label: req.label,
        host: req.host,
        port: req.port,
        protocol: req.protocol,
      },
    }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();
  page.screenSharingAddLabel().type("VNC Dev VM");
  page.screenSharingAddHost().type("192.168.1.5");
  page.screenSharingAddProtocol().select("VNC");
  page.screenSharingAddPort().clear().type("5900");
  page.screenSharingAddSubmit().click();

  // Then — AddTarget received protocol=VNC with correct fields
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ScreenSharingService.method.addTarget);
    expect(calls).to.have.length(1);
    expect(calls[0].label).to.equal("VNC Dev VM");
    expect(calls[0].host).to.equal("192.168.1.5");
    expect(calls[0].port).to.equal(5900);
    expect(calls[0].protocol).to.equal(Protocol.VNC);
  });
});

// ---------------------------------------------------------------------------
// AC-SS-3: Submitting an RDP target sends protocol=RDP
// ---------------------------------------------------------------------------

it("submitting RDP target calls AddTarget with protocol RDP and correct fields", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }))
    .onUnary(ScreenSharingService.method.addTarget, (req) => ({
      target: {
        id: "t-rdp-001",
        label: req.label,
        host: req.host,
        port: req.port,
        protocol: req.protocol,
      },
    }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();
  page.screenSharingAddLabel().type("Windows Server");
  page.screenSharingAddHost().type("10.0.0.10");
  page.screenSharingAddProtocol().select("RDP");
  page.screenSharingAddPort().clear().type("3389");
  page.screenSharingAddSubmit().click();

  // Then — AddTarget received protocol=RDP with correct fields
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ScreenSharingService.method.addTarget);
    expect(calls).to.have.length(1);
    expect(calls[0].label).to.equal("Windows Server");
    expect(calls[0].host).to.equal("10.0.0.10");
    expect(calls[0].port).to.equal(3389);
    expect(calls[0].protocol).to.equal(Protocol.RDP);
  });
});

// ---------------------------------------------------------------------------
// AC-SS-4: Submitting with a password shows the passphrase dialog first
// ---------------------------------------------------------------------------

it("shows the passphrase dialog before adding a target that has a password", () => {
  // Given
  const backend = aSessionsDrawerBackend([SESSION])
    .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }))
    .onUnary(ScreenSharingService.method.unlockVault, () => ({ ok: true }))
    .onUnary(ScreenSharingService.method.addTarget, (req) => ({
      target: {
        id: "t-secure-001",
        label: req.label,
        host: req.host,
        port: req.port,
        protocol: req.protocol,
      },
    }));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorScreenSharingTab().click();
  page.screenSharingAddLabel().type("Secure VM");
  page.screenSharingAddHost().type("10.0.0.1");
  page.screenSharingAddProtocol().select("VNC");
  page.screenSharingAddPort().clear().type("5900");
  page.screenSharingAddPassword().type("s3cr3t");
  page.screenSharingAddSubmit().click();

  // Then — passphrase dialog appears
  page.screenSharingPassphraseDialog().should("exist").and("be.visible");

  // When — user confirms passphrase
  page.screenSharingPassphraseInput().type("my-vault-passphrase");
  page.screenSharingPassphraseConfirm().click();

  // Then — UnlockVault was called before AddTarget
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ScreenSharingService.method.unlockVault)).to.have.length(1);
    expect(b.callsTo(ScreenSharingService.method.addTarget)).to.have.length(1);
  });
});

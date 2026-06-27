/**
 * Cypress component tests: SessionTrafficStrip — top-toolbar structural invariants
 *
 * RED PHASE — these tests define the expected DOM structure where the traffic
 * strip is rendered as a top toolbar OUTSIDE `sessions-detail-pane`, so no
 * absolutely-positioned overlay can ever cover it.
 *
 * All tests in this file FAIL until the production code is refactored:
 *   - Move `SessionTrafficStrip` rendering out of `SessionMainPane` (where it
 *     lives as a child of `sessions-detail-pane`) and into `SessionsDrawerScreen`
 *     as a sibling that precedes `SessionMainPane` / `sessions-detail-pane`.
 *
 * After the GREEN phase the failing assertion in each test will be:
 *   `sessionsDrawerPage.detailPane().find(strip)` → not.exist
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { interceptConnectionRpcs, interceptConnectSession } from "../support/rpc/connectionRpcs";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

const CONNECTED_SESSION = {
  sessionId: "traffic-topbar-aaaa-0000-0000-0000-000000000001",
  createdAt: "2026-06-26T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/topbar-test",
  pid: 52001,
  isActive: true,
  projectId: "proj-topbar-1",
  daemonInstanceId: "",
  workflowGoal: "Traffic strip topbar structural test",
  pendingElicitation: false,
};

// ---------------------------------------------------------------------------

describe("StatusBar — rendered as top toolbar, outside sessions-detail-pane", () => {
  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptConnectionRpcs([CONNECTED_SESSION]);
    // Use a LiveKit session to avoid GrpcSessionTerminal streaming RPCs in tests
    interceptConnectSession({ livekitRoom: "room-topbar-001", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });
  });

  // -------------------------------------------------------------------------
  // AC1: Strip exists at screen level
  // -------------------------------------------------------------------------

  it("renders the traffic strip somewhere in the sessions screen when session is connected", () => {
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();
    cy.wait("@connectSession");

    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
  });

  // -------------------------------------------------------------------------
  // AC2: Strip is NOT inside sessions-detail-pane
  //
  // FAILS currently: strip is a child of sessions-detail-pane (SessionMainPane.tsx:109)
  // PASSES after: strip moved to SessionsDrawerScreen level, above SessionMainPane
  // -------------------------------------------------------------------------

  it("does NOT render the traffic strip inside sessions-detail-pane", () => {
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();
    cy.wait("@connectSession");

    // Strip must exist somewhere in the screen...
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");

    // ...but NOT as a descendant of sessions-detail-pane.
    // This fails today: strip IS inside sessions-detail-pane.
    sessionsDrawerPage
      .detailPane()
      .find(`[data-testid="${TEST_IDS.sessionTrafficStrip}"]`)
      .should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC3: Strip precedes sessions-detail-pane in DOM order (not contained by it)
  //
  // FAILS currently: strip is a descendant of the pane, so compareDocumentPosition
  //   returns CONTAINS | PRECEDING (10), not FOLLOWING (4).
  // PASSES after: strip is a sibling before the pane — pane FOLLOWS strip (4).
  // -------------------------------------------------------------------------

  it("places the traffic strip before sessions-detail-pane in DOM order as a sibling, not a descendant", () => {
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();
    cy.wait("@connectSession");

    // Wait for the strip to be present before doing the DOM comparison
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");

    byTestId(TEST_IDS.sessionsDrawerScreen).then(($screen) => {
      const stripEl = $screen[0].querySelector(
        `[data-testid="${TEST_IDS.sessionTrafficStrip}"]`,
      );
      const paneEl = $screen[0].querySelector(
        `[data-testid="${TEST_IDS.sessionsDetailPane}"]`,
      );

      expect(stripEl, "traffic strip must be present in the screen").to.not.be.null;
      expect(paneEl, "sessions-detail-pane must be present in the screen").to.not.be.null;

      // DOCUMENT_POSITION_FOLLOWING (4): paneEl comes AFTER stripEl in document order.
      // When strip is INSIDE pane this returns CONTAINS | PRECEDING (10), not 4.
      const position = stripEl!.compareDocumentPosition(paneEl!);
      expect(
        position & Node.DOCUMENT_POSITION_FOLLOWING,
        "sessions-detail-pane must follow (not contain) the traffic strip in document order — strip must be a preceding sibling, not a descendant",
      ).to.not.equal(0);
    });
  });

  // -------------------------------------------------------------------------
  // AC4: Strip stays outside sessions-detail-pane when the inspector is expanded
  //
  // FAILS currently: strip is inside sessions-detail-pane regardless of inspector state.
  // PASSES after: strip is at a higher level, unaffected by the inspector's DOM position.
  // -------------------------------------------------------------------------

  it("remains outside sessions-detail-pane when the inspector is expanded to full screen", () => {
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();
    cy.wait("@connectSession");

    // Wait for the attachment useEffect to settle (connected → inspector auto-closes)
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");

    // Open the inspector, then expand it to full screen
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorExpand().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "expanded");

    // Strip must still exist...
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");

    // ...and still NOT be inside sessions-detail-pane, even with the inspector expanded.
    // This fails today: strip is a child of sessions-detail-pane regardless of inspector state.
    sessionsDrawerPage
      .detailPane()
      .find(`[data-testid="${TEST_IDS.sessionTrafficStrip}"]`)
      .should("not.exist");
  });
});

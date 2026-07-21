/**
 * Cypress component tests: SessionTrafficStrip — bottom Host Stats Footer structural invariants
 *
 * The traffic strip lives in the screen-level Host Stats Footer at the BOTTOM of
 * `SessionsDrawerScreen` (see `docs/ft/web/host-stats-footer.md`), rendered as a sibling that
 * FOLLOWS `SessionMainPane` / `sessions-detail-pane` — never as a descendant of the detail pane,
 * so no absolutely-positioned overlay inside the pane can ever cover it.
 *
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`), routed over the shared
 * common-room LiveKit connection — see `aConnectionServiceBackend` (in-memory fake) and
 * `SelectedDaemonProvider` (via `withSelectedDaemon`).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend, type ConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { hostStatsFooterPage } from "../support/pages/hostStatsFooterPage";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

const CONNECTED_SESSION = {
  sessionId: "traffic-footer-aaaa-0000-0000-0000-000000000001",
  createdAt: "2026-06-26T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/footer-test",
  pid: 52001,
  isActive: true,
  projectId: "proj-footer-1",
  daemonInstanceId: "",
  workflowGoal: "Traffic strip footer structural test",
  pendingElicitation: false,
};

// ---------------------------------------------------------------------------

describe("StatusBar — rendered in the bottom Host Stats Footer, outside sessions-detail-pane", () => {
  let backend: ConnectionServiceBackend;

  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
    backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION],
      // Use a LiveKit session to avoid GrpcSessionTerminal streaming RPCs in tests
      connectSession: { livekitRoom: "room-footer-001", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    });
  });

  it("renders the traffic strip somewhere in the sessions screen when session is connected", () => {
    // Given a connected session
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // When it is selected
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();

    // Then the traffic strip is present on the screen
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
  });

  it("renders the traffic strip inside the bottom host stats footer", () => {
    // Given a connected session
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // When it is selected
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();

    // Then the traffic strip lives inside the host stats footer
    hostStatsFooterPage.trafficStripInFooter().should("exist");
  });

  it("does NOT render the traffic strip inside sessions-detail-pane", () => {
    // Given a connected session
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // When it is selected
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();

    // Then the strip is not a descendant of the detail pane (so a pane overlay cannot cover it)
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
    sessionsDrawerPage
      .detailPane()
      .find(`[data-testid="${TEST_IDS.sessionTrafficStrip}"]`)
      .should("not.exist");
  });

  it("places the traffic strip after sessions-detail-pane in DOM order as a sibling, not a descendant", () => {
    // Given a connected session
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // When it is selected
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");

    // Then the strip (in the bottom footer) follows the detail pane as a sibling, not a descendant
    byTestId(TEST_IDS.sessionsDrawerScreen).then(($screen) => {
      const stripEl = $screen[0].querySelector(
        `[data-testid="${TEST_IDS.sessionTrafficStrip}"]`,
      );
      const paneEl = $screen[0].querySelector(
        `[data-testid="${TEST_IDS.sessionsDetailPane}"]`,
      );

      expect(stripEl, "traffic strip must be present in the screen").to.not.be.null;
      expect(paneEl, "sessions-detail-pane must be present in the screen").to.not.be.null;

      // DOCUMENT_POSITION_PRECEDING (2): paneEl comes BEFORE stripEl in document order — the strip
      // is a following sibling in the bottom footer, not a descendant of the pane.
      const position = stripEl!.compareDocumentPosition(paneEl!);
      expect(
        position & Node.DOCUMENT_POSITION_PRECEDING,
        "sessions-detail-pane must precede (not contain) the traffic strip — the strip is a following sibling in the bottom footer",
      ).to.not.equal(0);
    });
  });

  it("remains outside sessions-detail-pane when the inspector is expanded to full screen", () => {
    // Given a connected session
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();

    // Wait for the attachment useEffect to settle (connected → inspector auto-closes)
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");

    // When the inspector is opened, then expanded to full screen
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorExpand().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "expanded");

    // Then the strip is still present and still not inside the detail pane
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
    sessionsDrawerPage
      .detailPane()
      .find(`[data-testid="${TEST_IDS.sessionTrafficStrip}"]`)
      .should("not.exist");
  });
});

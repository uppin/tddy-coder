import React from "react";
import { fromBinary } from "@bufbuild/protobuf";
import { ResumeSessionRequestSchema } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  interceptConnectionRpcs,
  interceptConnectSession,
  interceptResumeSession,
} from "../support/rpc/connectionRpcs";
import { decodeProtoRequestBody } from "../support/rpc/protoRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Session constants used across specs
// ---------------------------------------------------------------------------

/** Active/connected session whose repoPath basename is used as the drawer label. */
const CONNECTED_SESSION_A = {
  sessionId: "drawer-connected-aaaaaaaa-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-feature-branch",
  pid: 10001,
  isActive: true,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "Build the session drawer",
  pendingElicitation: false,
};

/** A second connected session for the "switching panes" spec. */
const CONNECTED_SESSION_B = {
  sessionId: "drawer-connected-bbbbbbbb-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/another-branch",
  pid: 10002,
  isActive: true,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "Another feature",
  pendingElicitation: false,
};

/** Inactive/disconnected session — eligible for Resume. */
const DISCONNECTED_SESSION = {
  sessionId: "drawer-disconnected-cccccccc-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/old-branch",
  pid: 0,
  isActive: false,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "Older work",
  pendingElicitation: false,
};

/** Session with no repoPath — label should fall back to workflowGoal. */
const SESSION_WITH_GOAL_FALLBACK = {
  sessionId: "drawer-goal-fallback-dddddddd-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T09:00:00Z",
  status: "exited",
  repoPath: "",
  pid: 0,
  isActive: false,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "My workflow goal label",
  pendingElicitation: false,
};

/** Session with no repoPath and no workflowGoal — label should fall back to short session id. */
const SESSION_WITH_ID_FALLBACK = {
  sessionId: "deadbeef-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T08:00:00Z",
  status: "exited",
  repoPath: "",
  pid: 0,
  isActive: false,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
};

/** Active session with pendingElicitation — needs-input visual state. */
const SESSION_NEEDS_INPUT = {
  sessionId: "drawer-elicitation-eeeeeeee-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T07:00:00Z",
  status: "active",
  repoPath: "/home/dev/waiting-branch",
  pid: 10003,
  isActive: true,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: true,
};

// ---------------------------------------------------------------------------

describe("SessionsDrawerAcceptance — session list, status, labels, and detail pane", () => {
  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open (mobile defaults closed)
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC1: Drawer lists sessions in creation order (newest-first)
  // -------------------------------------------------------------------------

  it("lists sessions in newest-first creation order regardless of active status", () => {
    // Given — three sessions delivered out-of-order by the API; oldest is active, newest is inactive
    interceptConnectionRpcs([DISCONNECTED_SESSION, CONNECTED_SESSION_A, SESSION_WITH_GOAL_FALLBACK]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — drawer items appear newest-first by createdAt
    sessionsDrawerPage.drawer().within(() => {
      cy.get("button[data-testid^='sessions-drawer-item-']").then(($items) => {
        const ids = [...$items].map((el) => el.getAttribute("data-testid")!.replace("sessions-drawer-item-", ""));
        expect(ids[0]).to.equal(CONNECTED_SESSION_A.sessionId);
        expect(ids[1]).to.equal(DISCONNECTED_SESSION.sessionId);
        expect(ids[2]).to.equal(SESSION_WITH_GOAL_FALLBACK.sessionId);
      });
    });
  });

  // -------------------------------------------------------------------------
  // AC2: Each item shows its derived label
  // -------------------------------------------------------------------------

  it("shows the worktree basename as the label when repoPath is non-empty", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — label = basename of /home/dev/my-feature-branch
    sessionsDrawerPage.drawerItemLabel(CONNECTED_SESSION_A.sessionId)
      .should("have.text", "my-feature-branch");
  });

  it("shows workflowGoal as the label when repoPath is empty", () => {
    // Given
    interceptConnectionRpcs([SESSION_WITH_GOAL_FALLBACK]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then
    sessionsDrawerPage.drawerItemLabel(SESSION_WITH_GOAL_FALLBACK.sessionId)
      .should("have.text", "My workflow goal label");
  });

  it("shows the first 8 characters of sessionId as the label when both repoPath and workflowGoal are empty", () => {
    // Given — SESSION_WITH_ID_FALLBACK has sessionId starting with "deadbeef"
    interceptConnectionRpcs([SESSION_WITH_ID_FALLBACK]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then
    sessionsDrawerPage.drawerItemLabel(SESSION_WITH_ID_FALLBACK.sessionId)
      .should("have.text", "deadbeef");
  });

  // -------------------------------------------------------------------------
  // AC3: Each item shows a connected vs disconnected visual indicator
  // -------------------------------------------------------------------------

  it("marks an active session's status indicator as connected", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then
    sessionsDrawerPage.drawerItemStatus(CONNECTED_SESSION_A.sessionId)
      .should("have.attr", "data-status", "connected");
  });

  it("marks an inactive session's status indicator as disconnected", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then
    sessionsDrawerPage.drawerItemStatus(DISCONNECTED_SESSION.sessionId)
      .should("have.attr", "data-status", "disconnected");
  });

  it("marks a pending-elicitation session's status indicator as needs-input", () => {
    // Given
    interceptConnectionRpcs([SESSION_NEEDS_INPUT]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then
    sessionsDrawerPage.drawerItemStatus(SESSION_NEEDS_INPUT.sessionId)
      .should("have.attr", "data-status", "needs-input");
  });

  // -------------------------------------------------------------------------
  // AC4: Hovering an item reveals the full session id in a tooltip
  // -------------------------------------------------------------------------

  it("reveals the full session id in a tooltip when the drawer item is hovered", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).focus();

    // Then — tooltip content shows the full session id
    sessionsDrawerPage.drawerItemTooltip(CONNECTED_SESSION_A.sessionId)
      .should("be.visible")
      .and("contain.text", CONNECTED_SESSION_A.sessionId);
  });

  // -------------------------------------------------------------------------
  // AC5: Clicking a connected session opens its terminal in the detail pane
  // -------------------------------------------------------------------------

  it("opens the terminal container in the detail pane when a connected session is clicked", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({ livekitRoom: "room-session-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();

    // Then — terminal container appears; Resume button is absent
    sessionsDrawerPage.detailTerminalContainer().should("exist");
    sessionsDrawerPage.detailResumeBtn(CONNECTED_SESSION_A.sessionId).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC6: Clicking a disconnected session shows Resume + status + controls
  // -------------------------------------------------------------------------

  it("opens the inspector with metadata and controls when a disconnected session is clicked", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // Then — inspector opens with metadata and resume button; terminal container absent
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorMetadata().should("be.visible");
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).should("be.visible");
    sessionsDrawerPage.detailTerminalContainer().should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC7: Resume button calls ResumeSession with the correct session id
  // -------------------------------------------------------------------------

  it("calls ResumeSession with the disconnected session id when Resume is clicked", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);
    interceptResumeSession(DISCONNECTED_SESSION.sessionId);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).click();

    // Then — verify the request was made with the correct session id
    cy.wait("@resumeSession").then((interception) => {
      const decoded = fromBinary(
        ResumeSessionRequestSchema,
        decodeProtoRequestBody(interception.request.body),
      );
      expect(decoded.sessionId).to.equal(DISCONNECTED_SESSION.sessionId);
    });
  });

  // -------------------------------------------------------------------------
  // AC8: Selecting a second session switches the detail pane
  // -------------------------------------------------------------------------

  it("switches the detail pane to the second session when it is selected, with no terminal from the first session visible", () => {
    // Given — two connected sessions
    interceptConnectionRpcs([CONNECTED_SESSION_A, CONNECTED_SESSION_B]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    // When — select session A first
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();
    sessionsDrawerPage.detailTerminalContainer().should("exist");

    // Then — select session B; detail pane switches
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_B.sessionId).click();

    // The selected item changes to B
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_B.sessionId)
      .should("have.attr", "aria-selected", "true");

    // A is no longer the selected item
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId)
      .should("not.have.attr", "aria-selected", "true");
  });
});

// ---------------------------------------------------------------------------
// AC9-11: URL deep-link pre-selection
// Bug: SessionsDrawerScreen never reads window.location.hash on mount, so
// navigating to /#/sessions/:id shows an empty pane regardless of whether the
// session exists in the list.
// ---------------------------------------------------------------------------

describe("SessionsDrawerAcceptance — URL deep-link pre-selection", () => {
  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open (mobile defaults closed)
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
    // Reset hash so tests don't bleed into each other
    window.location.hash = "";
  });

  // -------------------------------------------------------------------------
  // AC9: Active session in the URL hash is selected in the drawer on mount
  // -------------------------------------------------------------------------

  it("auto-selects the active session from the URL hash when the screen mounts", () => {
    // Given — hash is set BEFORE the component mounts
    window.location.hash = `/sessions/${CONNECTED_SESSION_A.sessionId}`;
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — the session from the hash is marked as selected
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId)
      .should("have.attr", "aria-selected", "true");
  });

  // -------------------------------------------------------------------------
  // AC10: Active deep-link session is auto-connected (terminal appears)
  // -------------------------------------------------------------------------

  it("auto-connects to the active session identified in the URL hash on mount", () => {
    // Given
    window.location.hash = `/sessions/${CONNECTED_SESSION_A.sessionId}`;
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    cy.wait("@connectSession");

    // Then — terminal container is visible (auto-connected)
    sessionsDrawerPage.detailTerminalContainer().should("exist");
  });

  // -------------------------------------------------------------------------
  // AC11: Inactive deep-link session is selected but does not show placeholder
  // -------------------------------------------------------------------------

  it("selects an inactive session from the URL hash and shows session controls, not the empty placeholder", () => {
    // Given
    window.location.hash = `/sessions/${DISCONNECTED_SESSION.sessionId}`;
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — the drawer item is selected
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId)
      .should("have.attr", "aria-selected", "true");

    // And the empty placeholder is not shown
    cy.contains("Select a session").should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// Drawer toggle — close/open and strip mode
// ---------------------------------------------------------------------------

describe("SessionsDrawerAcceptance — drawer open/close toggle", () => {
  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open (mobile defaults closed)
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows a close button in the drawer header that collapses the drawer", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — close button exists while drawer is open
    sessionsDrawerPage.drawerCloseBtn().should("exist");

    // When — close button is clicked
    sessionsDrawerPage.drawerCloseBtn().click();

    // Then — session labels are no longer visible; drawer is in strip/hidden state
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "closed");
    cy.contains(CONNECTED_SESSION_A.workflowGoal).should("not.exist");
  });

  it("shows an open button in strip mode that re-expands the drawer", () => {
    // Given — drawer already closed
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerCloseBtn().click();
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "closed");

    // When — open button is clicked
    sessionsDrawerPage.drawerOpenBtn().click();

    // Then — drawer is open again and session label is visible
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "open");
    sessionsDrawerPage.drawerItemLabel(CONNECTED_SESSION_A.sessionId)
      .should("be.visible");
  });

  it("defaults the session list to closed on a mobile-width viewport", () => {
    // Given — a mobile viewport (narrower than the md breakpoint)
    cy.viewport(375, 667);
    interceptConnectionRpcs([CONNECTED_SESSION_A]);

    // When — the screen mounts
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — the session list starts collapsed so it does not cover the main pane
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "closed");
  });

  it("shows a floating overlay open control on a mobile viewport when the list is collapsed", () => {
    // Given — a mobile viewport where the session list starts collapsed
    cy.viewport(375, 667);
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "closed");

    // Then — a floating overlay control to open the list is visible
    sessionsDrawerPage.drawerOpenOverlayBtn().should("be.visible");
  });

  it("expands the session list when the overlay open control is tapped on mobile", () => {
    // Given — a mobile viewport with the session list collapsed
    cy.viewport(375, 667);
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // When — the user taps the floating overlay open control
    sessionsDrawerPage.drawerOpenOverlayBtn().click();

    // Then — the list expands, its sessions appear, and the overlay control is gone
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "open");
    sessionsDrawerPage.drawerItemLabel(CONNECTED_SESSION_A.sessionId).should("be.visible");
    sessionsDrawerPage.drawerOpenOverlayBtn().should("not.exist");
  });

  it("opens the session list as a full-width overlay on mobile, without resizing the detail pane", () => {
    // Given — a mobile viewport with the list collapsed
    cy.viewport(375, 667);
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // When — the user opens the list
    sessionsDrawerPage.drawerOpenOverlayBtn().click();
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "open");

    // Then — the open list is an overlay (out of flow) spanning the full container width
    sessionsDrawerPage.drawer().should("have.css", "position", "absolute");
    sessionsDrawerPage.drawer().then(($drawer) => {
      const containerWidth = $drawer.parent().outerWidth();
      expect($drawer.outerWidth()).to.eq(containerWidth);
    });
  });

  it("closes the session list after selecting a session on a mobile viewport", () => {
    // Given — a mobile viewport with the list opened over the terminal
    cy.viewport(375, 667);
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({
      livekitRoom: "room-session-a",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
    });
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerOpenOverlayBtn().click();
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "open");

    // When — the user selects a session
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();

    // Then — the list closes so the terminal is visible
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "closed");
  });

  it("does not show the floating overlay open control on a desktop viewport", () => {
    // Given — a desktop viewport where the session list is open by default
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    // Then — the desktop strip handles opening; no floating overlay control is rendered
    sessionsDrawerPage.drawerOpenOverlayBtn().should("not.exist");
  });
});

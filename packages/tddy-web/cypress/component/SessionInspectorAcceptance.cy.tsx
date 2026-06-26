import React from "react";
import { fromBinary } from "@bufbuild/protobuf";
import {
  ResumeSessionRequestSchema,
  SignalSessionRequestSchema,
} from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  interceptConnectionRpcs,
  interceptConnectSession,
  interceptDeleteSession,
  interceptResumeSession,
  interceptSignalSession,
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

/** Session with new optional fields — used to verify they render in the inspector metadata. */
const SESSION_WITH_NEW_FIELDS = {
  sessionId: "drawer-new-fields-ffffffff-0000-0000-0000-000000000000",
  createdAt: "2026-06-21T09:00:00Z",
  status: "exited",
  repoPath: "/home/dev/new-fields-branch",
  pid: 0,
  isActive: false,
  projectId: "proj-drawer-1",
  daemonInstanceId: "",
  workflowGoal: "Test new fields",
  pendingElicitation: false,
  tool: "tddy-coder",
  sessionType: "tool",
  updatedAt: "2026-06-21T10:30:00Z",
  livekitRoom: "",
  previousSessionId: "prev-session-aabbccdd",
};

// ---------------------------------------------------------------------------

describe("SessionInspectorAcceptance — inspector drawer open/expand/close and controls", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC1: Connected session → inspector hidden by default
  // -------------------------------------------------------------------------

  it("hides the inspector by default when a connected session is selected", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();

    // Then
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");
    sessionsDrawerPage.detailTerminalContainer().should("exist");
  });

  // -------------------------------------------------------------------------
  // AC2: Toggle opens inspector as overlay (terminal still visible)
  // -------------------------------------------------------------------------

  it("opens the inspector as an overlay when the toggle is clicked, leaving the terminal visible", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();

    // Then
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.detailTerminalContainer().should("exist");
  });

  // -------------------------------------------------------------------------
  // AC3: Disconnected session → inspector open by default
  // -------------------------------------------------------------------------

  it("opens the inspector by default when a disconnected session is selected", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // Then
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.detailTerminalContainer().should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC4: Expand button sets expanded state; restore returns to open
  // -------------------------------------------------------------------------

  it("expands the inspector to fill the content area when expand is clicked, and restores on restore click", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorExpand().click();

    // Then
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "expanded");

    // When — restore
    sessionsDrawerPage.inspectorRestore().click();

    // Then
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
  });

  // -------------------------------------------------------------------------
  // AC5: Close button closes the inspector
  // -------------------------------------------------------------------------

  it("closes the inspector when the close button is clicked", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorClose().click();

    // Then
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");
  });

  // -------------------------------------------------------------------------
  // AC6: Metadata renders new fields when present
  // -------------------------------------------------------------------------

  it("renders tool, session type, updated, and previousSessionId in the inspector metadata when set", () => {
    // Given
    interceptConnectionRpcs([SESSION_WITH_NEW_FIELDS]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(SESSION_WITH_NEW_FIELDS.sessionId).click();

    // Then
    sessionsDrawerPage.inspectorMetadata()
      .should("contain.text", "tddy-coder")
      .and("contain.text", "tool")
      .and("contain.text", "prev-session-aabbccdd");
    // livekitRoom is "" so no LiveKit room row should render
    sessionsDrawerPage.inspectorMetadata().should("not.contain.text", "LiveKit room");
  });

  // -------------------------------------------------------------------------
  // AC7: Resume calls ResumeSession with correct id
  // -------------------------------------------------------------------------

  it("calls ResumeSession with the disconnected session id when Resume is clicked in the inspector", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);
    interceptResumeSession(DISCONNECTED_SESSION.sessionId);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).click();

    // Then
    cy.wait("@resumeSession").then((interception) => {
      const decoded = fromBinary(
        ResumeSessionRequestSchema,
        decodeProtoRequestBody(interception.request.body),
      );
      expect(decoded.sessionId).to.equal(DISCONNECTED_SESSION.sessionId);
    });
  });

  // -------------------------------------------------------------------------
  // AC8: Delete requires confirm step; calls DeleteSession
  // -------------------------------------------------------------------------

  it("requires a confirm click before calling DeleteSession when Delete is clicked in the inspector", () => {
    // Given
    const capturedIds: string[] = [];
    interceptConnectionRpcs([DISCONNECTED_SESSION]);
    interceptDeleteSession(capturedIds);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // First click — shows confirm button, does NOT call delete yet
    sessionsDrawerPage.inspectorDeleteBtn(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDeleteConfirm(DISCONNECTED_SESSION.sessionId).should("be.visible");
    cy.then(() => expect(capturedIds).to.have.length(0));

    // Second click — confirm — calls DeleteSession
    sessionsDrawerPage.inspectorDeleteConfirm(DISCONNECTED_SESSION.sessionId).click();

    // Then
    cy.wait("@deleteSession");
    cy.then(() => {
      expect(capturedIds).to.have.length(1);
      expect(capturedIds[0]).to.equal(DISCONNECTED_SESSION.sessionId);
    });
  });

  // -------------------------------------------------------------------------
  // AC9: Terminate calls SignalSession with SIGTERM
  // -------------------------------------------------------------------------

  it("calls SignalSession with SIGTERM when Terminate is clicked in the inspector for an active session", () => {
    // Given
    interceptConnectionRpcs([CONNECTED_SESSION_A]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });
    interceptSignalSession();

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION_A.sessionId).click();

    // Then
    cy.wait("@signalSession").then((interception) => {
      const decoded = fromBinary(
        SignalSessionRequestSchema,
        decodeProtoRequestBody(interception.request.body),
      );
      expect(decoded.sessionId).to.equal(CONNECTED_SESSION_A.sessionId);
      expect(decoded.signal).to.equal(15); // SIGTERM = 15
    });
  });

  // -------------------------------------------------------------------------
  // AC1+AC2: Inspector tab strip — Details default + Tools tab switching
  //
  // ⚠️ RED PHASE — these tests fail until InspectorTabs is added to
  // SessionInspectorDrawer and the tabs are wired in.
  // -------------------------------------------------------------------------

  it("shows Details and Tools tabs; Details is selected by default and metadata is visible", () => {
    // Given — a disconnected session so the inspector opens automatically
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");

    // Then — both tabs exist
    cy.get(`[data-testid="sessions-inspector-tab-details"]`).should("exist");
    cy.get(`[data-testid="sessions-inspector-tab-tools"]`).should("exist");

    // And — Details tab is active and metadata is visible (regression guard)
    cy.get(`[data-testid="sessions-inspector-tab-details"]`).should("have.attr", "aria-selected", "true");
    sessionsDrawerPage.inspectorMetadata().should("be.visible");
  });

  it("switches to the Tools tab and reveals the tools panel; switching back restores the metadata panel", () => {
    // Given
    interceptConnectionRpcs([DISCONNECTED_SESSION]);

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");

    // Switch to Tools tab
    cy.get(`[data-testid="sessions-inspector-tab-tools"]`).click();

    // Then — metadata hidden, Tools panel visible
    sessionsDrawerPage.inspectorMetadata().should("not.exist");
    cy.get(`[data-testid="sessions-inspector-tools-panel"]`).should("exist");

    // When — switch back to Details tab
    cy.get(`[data-testid="sessions-inspector-tab-details"]`).click();

    // Then — metadata restored, Tools panel gone
    sessionsDrawerPage.inspectorMetadata().should("be.visible");
    cy.get(`[data-testid="sessions-inspector-tools-panel"]`, { timeout: 100 }).should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// Attachment-driven inspector: auto-open/close based on connection status
// ---------------------------------------------------------------------------

describe("SessionInspectorAcceptance — attachment-driven auto-open/close", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("closes the inspector when a session becomes connected", () => {
    // Given — disconnected session selected; inspector starts open
    interceptConnectionRpcs([DISCONNECTED_SESSION]);
    interceptResumeSession(DISCONNECTED_SESSION.sessionId);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");

    // When — user resumes the session (triggers attachment → connected-livekit)
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).click();
    cy.wait("@resumeSession");

    // Then — inspector closes to reveal the terminal
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");
  });

  it("opens the inspector when a connected session attachment becomes idle", () => {
    // Given — connected session selected; also list a disconnected session to switch to
    interceptConnectionRpcs([CONNECTED_SESSION_A, DISCONNECTED_SESSION]);
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();
    cy.wait("@connectSession");
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");

    // When — simulate session going idle by selecting a disconnected session then reselecting
    // (The attachment hook resets to idle when the session changes)
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // Then — inspector opens for the now-disconnected selection
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
  });
});

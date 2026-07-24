/**
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`), routed over the shared
 * common-room LiveKit connection — see `aConnectionServiceBackend` (in-memory fake) and
 * `SelectedDaemonProvider` (via `withSelectedDaemon`).
 */

import React from "react";
import { ConnectionService, Signal } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
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
    cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC1: Connected session → inspector hidden by default
  // -------------------------------------------------------------------------

  it("hides the inspector by default when a connected session is selected", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION_A],
      connectSession: { livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION_A],
      connectSession: { livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({ sessions: [SESSION_WITH_NEW_FIELDS] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.resumeSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionId).to.equal(DISCONNECTED_SESSION.sessionId);
    });
  });

  // -------------------------------------------------------------------------
  // AC8: Delete requires confirm step; calls DeleteSession
  // -------------------------------------------------------------------------

  it("requires a confirm click before calling DeleteSession when Delete is clicked in the inspector", () => {
    // Given
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // First click — shows confirm button, does NOT call delete yet
    sessionsDrawerPage.inspectorDeleteBtn(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDeleteConfirm(DISCONNECTED_SESSION.sessionId).should("be.visible");
    cy.wrap(backend).should((b) => {
      expect(b.deletedSessionIds).to.have.length(0);
    });

    // Second click — confirm — calls DeleteSession
    sessionsDrawerPage.inspectorDeleteConfirm(DISCONNECTED_SESSION.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.deletedSessionIds).to.deep.equal([DISCONNECTED_SESSION.sessionId]);
    });
  });

  // -------------------------------------------------------------------------
  // AC9: Terminate calls SignalSession with SIGTERM
  // -------------------------------------------------------------------------

  it("calls SignalSession with SIGTERM when Terminate is clicked in the inspector for an active session", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION_A],
      connectSession: { livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION_A.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls).to.deep.equal([{ sessionId: CONNECTED_SESSION_A.sessionId, signal: Signal.SIGTERM }]);
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
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("closes the inspector when a session becomes connected", () => {
    // Given — disconnected session selected; inspector starts open
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");

    // When — user resumes the session (triggers attachment → connected-livekit)
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).click();

    // Then — inspector closes to reveal the terminal
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");
  });

  it("opens the inspector when a connected session attachment becomes idle", () => {
    // Given — connected session selected; also list a disconnected session to switch to
    const backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION_A, DISCONNECTED_SESSION],
      connectSession: { livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    });

    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(CONNECTED_SESSION_A.sessionId).click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "closed");

    // When — simulate session going idle by selecting a disconnected session then reselecting
    // (The attachment hook resets to idle when the session changes). The disconnected row lives in
    // the default-collapsed Remaining partition, so expand it first.
    sessionsDrawerPage.expandRemaining();
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // Then — inspector opens for the now-disconnected selection
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
  });
});

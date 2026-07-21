/**
 * Acceptance test for auto-closing the sessions drawer after a new session is created.
 *
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`), routed over the shared
 * common-room LiveKit connection — see `aConnectionServiceBackend` (in-memory fake) and
 * `SelectedDaemonProvider` (via `withSelectedDaemon`). The test mounts SessionsDrawerScreen
 * and drives the full create flow through the in-memory backend.
 *
 * Behaviour under test: once a session is successfully created, the left sessions drawer
 * collapses so the new session's terminal is unobstructed in the main pane.
 */
import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { TEST_IDS, byTestId } from "../support/testIds";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const NEW_SESSION_ID = "autoclose-session-bbbb-0000-0000-0000-000000000001";

/** Fixture returned by the second listSessions call after creation. */
const NEW_SESSION_FIXTURE = {
  sessionId: NEW_SESSION_ID,
  createdAt: "2026-06-26T12:10:00Z",
  status: "active",
  repoPath: "/home/dev/new-feature",
  pid: 20001,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
  workflowGoal: "New work",
  pendingElicitation: false,
};

/** Backend that starts with no sessions and returns the new one after creation. */
function backendThatCreates() {
  let callCount = 0;
  return aConnectionServiceBackend({
    projectsOverride: [{ projectId: "proj-1", name: "Test Project" }],
    agents: [{ id: "claude", label: "Claude (opus)" }],
    listSessionsFactory: () => {
      callCount++;
      return callCount > 1 ? [NEW_SESSION_FIXTURE] : [];
    },
    startSession: {
      sessionId: NEW_SESSION_ID,
      livekitRoom: `room-${NEW_SESSION_ID}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server-new",
    },
    connectSession: { livekitRoom: `room-${NEW_SESSION_ID}` },
  });
}

/** Fill the required create-form fields and submit. */
function createSession() {
  byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
  byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
  byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
  byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();
}

// ---------------------------------------------------------------------------

describe("CreateSession acceptance — drawer auto-close after creation", () => {
  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open, so the close is observable
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("auto-closes the sessions drawer after a new session is created", () => {
    // Given — desktop with the drawer open
    const backend = backendThatCreates();
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "open");

    // When — a new session is created
    createSession();

    // Then — the drawer collapses so the new session's terminal is unobstructed
    sessionsDrawerPage.drawer().should("have.attr", "data-drawer-state", "closed");
  });
});

/**
 * Cypress component acceptance: Fast Session Change — Start/Resume/Connect still route to the
 * daemon participant (regression guard for the bootstrap boundary).
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 1 boundary)
 *
 * Green: the session-scoped retargeting lands AND the bootstrap/lifecycle RPCs
 * (`StartSession` / `ConnectSession` / `ResumeSession` / `DeleteSession` / `SignalSession`) keep
 * targeting `daemon-<instanceId>`. Delete/signal are daemon-direct so they still work when the
 * coder participant is stuck.
 */

import React from "react";
import { ConnectionService } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ACTIVE_SESSION = {
  sessionId: "boundary-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-boundary",
  pid: 73001,
  isActive: true,
  projectId: "proj-boundary-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

const DISCONNECTED_SESSION = {
  sessionId: "boundary-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-12T08:00:00Z",
  status: "exited",
  repoPath: "/home/dev/old-boundary",
  pid: 0,
  isActive: false,
  projectId: "proj-boundary-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

const DAEMON_INSTANCE_ID = "local";
const DAEMON_PARTICIPANT_IDENTITY = `daemon-${DAEMON_INSTANCE_ID}`;

function aBackendForSessions() {
  return aConnectionServiceBackend({
    sessions: [ACTIVE_SESSION, DISCONNECTED_SESSION],
    connectSession: () => ({
      livekitRoom: `room-${ACTIVE_SESSION.sessionId}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: `daemon-local-${ACTIVE_SESSION.sessionId}`,
    }),
    resumeSession: () => ({
      sessionId: DISCONNECTED_SESSION.sessionId,
      livekitRoom: `room-${DISCONNECTED_SESSION.sessionId}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: `daemon-local-${DISCONNECTED_SESSION.sessionId}`,
    }),
  });
}

// ---------------------------------------------------------------------------

describe("SessionStartResumeStillDaemonRouted — bootstrap RPCs keep targeting the daemon participant", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("routes ResumeSession to the daemon participant identity, not the session participant", () => {
    // Given — a disconnected session shown in the drawer
    const backend = aBackendForSessions();
    const harness = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: DAEMON_INSTANCE_ID, label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // When — the user resumes the session
    sessionsDrawerPage.inspectorResumeBtn(DISCONNECTED_SESSION.sessionId).click();

    // Then — ResumeSession reached the backend and a LiveKit client was built for the daemon participant
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.resumeSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionId).to.equal(DISCONNECTED_SESSION.sessionId);
    });
    cy.wrap(harness).should((h) => {
      expect(h.targets).to.include(DAEMON_PARTICIPANT_IDENTITY);
    });
  });

  it("routes ConnectSession to the daemon participant identity when attaching to an active session", () => {
    // Given — an active session shown in the drawer
    const backend = aBackendForSessions();
    const harness = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: DAEMON_INSTANCE_ID, label: "local" }]),
      backend,
    );

    // When — the user clicks the active session row (attachment bootstrap = ConnectSession)
    sessionsDrawerPage.drawerItem(ACTIVE_SESSION.sessionId).click();

    // Then — ConnectSession reached the backend via the daemon participant identity
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.connectSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionId).to.equal(ACTIVE_SESSION.sessionId);
    });
    cy.wrap(harness).should((h) => {
      expect(h.targets).to.include(DAEMON_PARTICIPANT_IDENTITY);
    });
  });

  it("routes DeleteSession to the daemon participant identity, not the session participant", () => {
    // Given — an attached session with the inspector open
    const backend = aBackendForSessions();
    const harness = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: DAEMON_INSTANCE_ID, label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(ACTIVE_SESSION.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(ACTIVE_SESSION.sessionId).should("exist");
    sessionsDrawerPage.inspectorToggle().click();

    // When — the user terminates the active session (single-click Terminate for active sessions)
    sessionsDrawerPage.inspectorTerminateBtn(ACTIVE_SESSION.sessionId).click();

    // Then — SignalSession (terminate) reached the backend via the daemon participant identity,
    // not the session participant identity.
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls.map((c) => c.sessionId)).to.include(ACTIVE_SESSION.sessionId);
    });
    cy.wrap(harness).should((h) => {
      expect(h.targets).to.include(DAEMON_PARTICIPANT_IDENTITY);
      expect(h.targets).not.to.include(`daemon-local-${ACTIVE_SESSION.sessionId}`);
    });
  });
});

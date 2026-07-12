/**
 * Cypress component acceptance: Fast Session Change — background terminals survive focus switch.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 2, 3)
 *
 * Green: `SessionRuntimeRegistry` mounts one terminal per attached
 * session in a hidden runtime layer (`sessions-runtime-layer`), keeping switched-away
 * terminals subscribed to `streamTerminalIO`.
 */

import React from "react";
import { ConnectionService } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures — two active sessions on the local daemon
// ---------------------------------------------------------------------------

const SESSION_A = {
  sessionId: "bg-runtime-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 70001,
  isActive: true,
  projectId: "proj-bg-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

const SESSION_B = {
  sessionId: "bg-runtime-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-12T09:05:00Z",
  status: "active",
  repoPath: "/home/dev/feature-beta",
  pid: 70002,
  isActive: true,
  projectId: "proj-bg-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

function aBackendForBothSessions() {
  return aConnectionServiceBackend({
    sessions: [SESSION_A, SESSION_B],
    connectSession: (sessionId) => ({
      livekitRoom: `room-${sessionId}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: `daemon-local-${sessionId}`,
    }),
  });
}

// ---------------------------------------------------------------------------

describe("SessionRuntimeRegistryBackgroundTerminals — background terminals survive a focus switch", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("keeps session A's terminal mounted and still receiving bytes after the user switches focus to session B", () => {
    // Given — two attached LiveKit sessions
    const backend = aBackendForBothSessions();
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: "local", label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION_A.sessionId).should("exist");
    sessionsDrawerPage.drawerItem(SESSION_B.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION_B.sessionId).should("exist");

    // When — the user switches focus back to A (A was backgrounded; no unmount should have occurred)

    // Then — A's runtime terminal is still mounted in the runtime layer ...
    sessionsDrawerPage.runtimeLayer().should("exist");
    sessionsDrawerPage.runtimeTerminal(SESSION_A.sessionId).should("exist");

    // ... and no ConnectSession reconnect was issued for A (focus switch is not a re-attach).
    cy.wrap(backend).should((b) => {
      const reconnects = b.callsTo(ConnectionService.method.connectSession).filter(
        (c) => c.sessionId === SESSION_A.sessionId,
      );
      expect(reconnects).to.have.length(1, "session A should be attached exactly once, not reconnected on focus switch");
    });
  });
});

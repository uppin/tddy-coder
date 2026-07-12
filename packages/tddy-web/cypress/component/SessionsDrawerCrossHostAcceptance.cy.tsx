/**
 * Acceptance: the sessions drawer shows a session that has a live LiveKit participant regardless of
 * the selected host, and routes interaction with such a row to that session's owning daemon.
 *
 * Liveness comes from LiveKit participant presence (a session's coder joins the common room as
 * `daemon-<instance>-<sessionId>`), seeded here via `aFakeCommonRoom`. The drawer does NOT fan
 * `ListSessions` out — it queries only the selected host (for inactive/history rows) and derives
 * active cross-host rows from the room. Each host answers with its own backend
 * (`mountWithPerDaemonLiveKitRpc`) so a call landing on host B's backend proves routing to host B.
 */

import React from "react";
import { ConnectionService, Signal } from "../../src/gen/connection_pb";
import { daemonRpcIdentity, type DaemonHost } from "../../src/lib/participantRole";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { mountWithPerDaemonLiveKitRpc } from "../support/rpc/perDaemonLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Host + session fixtures (session ids are UUIDs — the coder identity encodes them)
// ---------------------------------------------------------------------------

/** Host A — selected first by `SelectedDaemonProvider`. */
const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
/** Host B — a peer daemon, never selected in these specs. */
const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

/** A session on host A (the selected host). */
const SESSION_A = {
  sessionId: "aaaaaaaa-0000-4000-8000-000000000001",
  createdAt: "2026-07-10T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/host-a-branch",
  pid: 30001,
  isActive: true,
  projectId: "proj-a",
  daemonInstanceId: HOST_A.instanceId,
  pendingElicitation: false,
};

/** A session on host B — shown while host A is selected only when its participant is live. */
const SESSION_B_ACTIVE = {
  sessionId: "bbbbbbbb-0000-4000-8000-000000000002",
  createdAt: "2026-07-10T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/host-b-live",
  pid: 30002,
  isActive: true,
  projectId: "proj-b",
  daemonInstanceId: HOST_B.instanceId,
  pendingElicitation: false,
};

/** A session on host B with no live participant — must never appear while host A is selected. */
const SESSION_C_INACTIVE = {
  sessionId: "cccccccc-0000-4000-8000-000000000003",
  createdAt: "2026-07-10T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/host-b-idle",
  pid: 0,
  isActive: false,
  projectId: "proj-b",
  daemonInstanceId: HOST_B.instanceId,
  pendingElicitation: false,
};

const LIVEKIT_CONNECT = {
  livekitRoom: "room-xhost",
  livekitUrl: "ws://127.0.0.1:7880",
  livekitServerIdentity: "server",
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** The common-room coder participant identity for a session on its owning host. */
function coderIdentityFor(session: { sessionId: string; daemonInstanceId: string }): string {
  return `daemon-${session.daemonInstanceId}-${session.sessionId}`;
}

/**
 * Mount the drawer with two hosts, each answering with its own backend. `liveSessions` are seeded as
 * live coder participants in the common room (so they count as active regardless of `ListSessions`).
 */
function mountCrossHost(
  backendA: ConnectionServiceBackend,
  backendB: ConnectionServiceBackend,
  liveSessions: Array<{ sessionId: string; daemonInstanceId: string }> = [],
) {
  mountWithPerDaemonLiveKitRpc(
    withSelectedDaemon(<SessionsDrawerScreen />, [HOST_A, HOST_B], liveSessions.map(coderIdentityFor)),
    {
      [daemonRpcIdentity(HOST_A.instanceId)]: backendA,
      [daemonRpcIdentity(HOST_B.instanceId)]: backendB,
    },
    { httpBackend: backendA },
  );
}

const listSessionsCount = (b: ConnectionServiceBackend) =>
  b.callsTo(ConnectionService.method.listSessions).length;

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("SessionsDrawerCrossHostAcceptance — active sessions across hosts", () => {
  beforeEach(() => {
    cy.viewport(1280, 800); // desktop: session list defaults open
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("queries only the selected host for the session list — no fan-out to other daemons", () => {
    // Given — host B has a live participant; host B's backend must never be asked for its list.
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aConnectionServiceBackend({ sessions: [SESSION_B_ACTIVE] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — the live cross-host row still shows, but only host A was queried for the list.
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).should("exist");
    cy.wrap(backendA).should((b: ConnectionServiceBackend) => {
      expect(listSessionsCount(b), "host A ListSessions calls").to.be.greaterThan(0);
    });
    cy.wrap(backendB).should((b: ConnectionServiceBackend) => {
      expect(listSessionsCount(b), "host B ListSessions calls (no fan-out)").to.equal(0);
    });
  });

  it("shows a host-B session with a live participant while host A is selected", () => {
    // Given
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aConnectionServiceBackend({ sessions: [] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — both the selected-host session and the live cross-host session are listed
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).should("exist");
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).should("exist");
  });

  it("does not show a host-B session that has no live participant", () => {
    // Given — host B has an inactive session and no live participant.
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aConnectionServiceBackend({ sessions: [SESSION_C_INACTIVE] });

    // When — nothing seeded live.
    mountCrossHost(backendA, backendB, []);

    // Then — the selected-host session shows; the idle cross-host session never appears.
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).should("exist");
    sessionsDrawerPage.drawerItem(SESSION_C_INACTIVE.sessionId).should("not.exist");
  });

  it("labels a cross-host row with its owning host, and does not label same-host rows", () => {
    // Given
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aConnectionServiceBackend({ sessions: [] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — the cross-host row carries a "server-2" host badge; the selected-host row has none
    sessionsDrawerPage.drawerItemHost(SESSION_B_ACTIVE.sessionId).should("have.text", "server-2");
    sessionsDrawerPage.drawerItemHost(SESSION_A.sessionId).should("not.exist");
  });

  it("routes ConnectSession to the owning host when a cross-host row is selected", () => {
    // Given
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A], connectSession: LIVEKIT_CONNECT });
    const backendB = aConnectionServiceBackend({ sessions: [], connectSession: LIVEKIT_CONNECT });

    // When — select the live host-B session
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).click();

    // Then — ConnectSession lands on host B (the owner), never on host A, and the terminal appears
    cy.wrap(backendB).should((b: ConnectionServiceBackend) => {
      const calls = b.callsTo(ConnectionService.method.connectSession);
      expect(calls, "backendB (host B) ConnectSession calls").to.have.length(1);
      expect(calls[0].sessionId).to.equal(SESSION_B_ACTIVE.sessionId);
    });
    cy.wrap(backendA).should((b: ConnectionServiceBackend) => {
      expect(
        b.callsTo(ConnectionService.method.connectSession),
        "backendA (host A) ConnectSession calls",
      ).to.have.length(0);
    });
    sessionsDrawerPage.detailTerminalContainer().should("exist");
  });

  it("does not switch the selected host when a cross-host row is selected", () => {
    // Given
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A], connectSession: LIVEKIT_CONNECT });
    const backendB = aConnectionServiceBackend({ sessions: [], connectSession: LIVEKIT_CONNECT });
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // When — select the cross-host row (connects to host B)
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).click();
    cy.wrap(backendB).should((b: ConnectionServiceBackend) => {
      expect(b.callsTo(ConnectionService.method.connectSession)).to.have.length(1);
    });

    // Then — the selected host is unchanged: host A's own session is still listed, and host A's
    // list was never re-queried against host B.
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).should("exist");
    cy.wrap(backendB).should((b: ConnectionServiceBackend) => {
      expect(listSessionsCount(b), "host B ListSessions calls (never became selected)").to.equal(0);
    });
  });

  it("routes a SIGTERM Terminate to the owning host for a cross-host live session", () => {
    // Given
    const backendA = aConnectionServiceBackend({ sessions: [SESSION_A], connectSession: LIVEKIT_CONNECT });
    const backendB = aConnectionServiceBackend({ sessions: [], connectSession: LIVEKIT_CONNECT });

    // When — select the live host-B session, open the inspector, and Terminate
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorTerminateBtn(SESSION_B_ACTIVE.sessionId).click();

    // Then — SignalSession(SIGTERM) lands on host B, never host A
    cy.wrap(backendB).should((b: ConnectionServiceBackend) => {
      expect(b.signalCalls, "backendB (host B) SignalSession calls").to.have.length(1);
      expect(b.signalCalls[0].sessionId).to.equal(SESSION_B_ACTIVE.sessionId);
      expect(b.signalCalls[0].signal).to.equal(Signal.SIGTERM);
    });
    cy.wrap(backendA).should((b: ConnectionServiceBackend) => {
      expect(b.signalCalls, "backendA (host A) SignalSession calls").to.have.length(0);
    });
  });

  it("still lists a legacy session with an empty daemonInstanceId on a single daemon", () => {
    // Given — one daemon, one session the daemon returns with an empty daemonInstanceId (legacy
    // local daemon). Regression guard for the owning-host attribution fallback.
    const legacySession = { ...SESSION_A, daemonInstanceId: "" };
    const backend = aConnectionServiceBackend({ sessions: [legacySession] });

    // When — mounted with the single default fixture daemon
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // Then — the session is listed (attributed to the selected daemon)
    sessionsDrawerPage.drawerItem(legacySession.sessionId).should("exist");
  });
});

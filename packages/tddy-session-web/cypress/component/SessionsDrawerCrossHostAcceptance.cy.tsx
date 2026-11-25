/**
 * Acceptance: the sessions drawer shows a session that has a live LiveKit participant regardless of
 * the selected host, and routes interaction with such a row to that session's owning daemon.
 *
 * Liveness comes from LiveKit participant presence (a session's coder joins the common room as
 * `daemon-<instance>-<sessionId>`), seeded here via `aFakeCommonRoom`. The browser asks only the
 * selected host for the list, with `includePeers`: that host answers for every host it is
 * connected to (the fan-out is the daemon's), so its backend here stands in for that whole answer —
 * an inactive session on another host is a row it lists naming that host. Active cross-host rows
 * also come from the room. Each host answers with its own backend (`mountWithPerDaemonLiveKitRpc`)
 * so a call landing on host B's backend proves routing to host B.
 */

import React from "react";
import { SessionService, Signal } from "tddy-client-web/gen/session_pb";
import { daemonRpcIdentity, type DaemonHost } from "tddy-client-web";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
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

/** A session on host B with no live participant — shown only when the selected host lists it. */
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
  backendA: SessionServiceBackend,
  backendB: SessionServiceBackend,
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

const listSessionsCount = (b: SessionServiceBackend) =>
  b.callsTo(SessionService.method.listSessions).length;

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

  it("asks the selected host for every connected host's sessions, and no other host", () => {
    // Given — host B has a live participant; host B's backend must never be asked for its list.
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aSessionServiceBackend({ sessions: [SESSION_B_ACTIVE] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — the live cross-host row still shows; host A was asked to include its peers, and the
    // browser never asked host B itself (host A fans out).
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).should("exist");
    cy.wrap(backendA).should((b: SessionServiceBackend) => {
      const calls = b.callsTo(SessionService.method.listSessions);
      expect(calls.length, "host A ListSessions calls").to.be.greaterThan(0);
      expect(
        calls.every((call) => call.includePeers),
        "every host A ListSessions call includes its peers",
      ).to.equal(true);
    });
    cy.wrap(backendB).should((b: SessionServiceBackend) => {
      expect(listSessionsCount(b), "host B ListSessions calls (the browser does not fan out)").to.equal(0);
    });
  });

  it("shows a host-B session with a live participant while host A is selected", () => {
    // Given
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aSessionServiceBackend({ sessions: [] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — both the selected-host session and the live cross-host session are listed
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).should("exist");
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).should("exist");
  });

  it("does not show a host-B session the selected host does not list and that has no live participant", () => {
    // Given — host B has an inactive session host A's answer does not carry (host B unreachable from
    // host A), and no live participant.
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aSessionServiceBackend({ sessions: [SESSION_C_INACTIVE] });

    // When — nothing seeded live.
    mountCrossHost(backendA, backendB, []);

    // Then — the selected-host session shows; the idle cross-host session does not.
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).should("exist");
    sessionsDrawerPage.drawerItem(SESSION_C_INACTIVE.sessionId).should("not.exist");
  });

  it("shows an inactive host-B session the selected host lists, labelled with host B", () => {
    // Given — host A's answer, fanned out to its peers, carries host B's idle session; no participant.
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A, SESSION_C_INACTIVE] });
    const backendB = aSessionServiceBackend({ sessions: [] });

    // When — nothing seeded live.
    mountCrossHost(backendA, backendB, []);

    // Then — the idle host-B session is listed, carrying host B's badge; host A's row carries none.
    sessionsDrawerPage.drawerItem(SESSION_C_INACTIVE.sessionId).should("exist");
    sessionsDrawerPage.drawerItemHost(SESSION_C_INACTIVE.sessionId).should("have.text", HOST_B.label);
    sessionsDrawerPage.expectNoOwningHostBadge(SESSION_A.sessionId);
  });

  it("lists a live host-B session once when the selected host also lists it", () => {
    // Given — host A's answer carries host B's session, and its participant is live in the room.
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A, SESSION_B_ACTIVE] });
    const backendB = aSessionServiceBackend({ sessions: [] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — one row for it, with host B's badge
    sessionsDrawerPage.drawerItemHost(SESSION_B_ACTIVE.sessionId).should("have.text", HOST_B.label);
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).should("have.length", 1);
  });

  it("labels a cross-host row with its owning host, and does not label same-host rows", () => {
    // Given
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A] });
    const backendB = aSessionServiceBackend({ sessions: [] });

    // When
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // Then — the cross-host row carries a "server-2" host badge; the selected-host row has none
    sessionsDrawerPage.drawerItemHost(SESSION_B_ACTIVE.sessionId).should("have.text", "server-2");
    sessionsDrawerPage.drawerItemHost(SESSION_A.sessionId).should("not.exist");
  });

  it("routes ConnectSession to the owning host when a cross-host row is selected", () => {
    // Given
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A], connectSession: LIVEKIT_CONNECT });
    const backendB = aSessionServiceBackend({ sessions: [], connectSession: LIVEKIT_CONNECT });

    // When — select the live host-B session
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).click();

    // Then — ConnectSession lands on host B (the owner), never on host A, and the terminal appears
    cy.wrap(backendB).should((b: SessionServiceBackend) => {
      const calls = b.callsTo(SessionService.method.connectSession);
      expect(calls, "backendB (host B) ConnectSession calls").to.have.length(1);
      expect(calls[0].sessionId).to.equal(SESSION_B_ACTIVE.sessionId);
    });
    cy.wrap(backendA).should((b: SessionServiceBackend) => {
      expect(
        b.callsTo(SessionService.method.connectSession),
        "backendA (host A) ConnectSession calls",
      ).to.have.length(0);
    });
    sessionsDrawerPage.detailTerminalContainer().should("exist");
  });

  it("does not switch the selected host when a cross-host row is selected", () => {
    // Given
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A], connectSession: LIVEKIT_CONNECT });
    const backendB = aSessionServiceBackend({ sessions: [], connectSession: LIVEKIT_CONNECT });
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);

    // When — select the cross-host row (connects to host B)
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).click();
    cy.wrap(backendB).should((b: SessionServiceBackend) => {
      expect(b.callsTo(SessionService.method.connectSession)).to.have.length(1);
    });

    // Then — the selected host is unchanged: host A's own session is still listed, and host A's
    // list was never re-queried against host B.
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).should("exist");
    cy.wrap(backendB).should((b: SessionServiceBackend) => {
      expect(listSessionsCount(b), "host B ListSessions calls (never became selected)").to.equal(0);
    });
  });

  it("routes a SIGTERM Terminate to the owning host for a cross-host live session", () => {
    // Given
    const backendA = aSessionServiceBackend({ sessions: [SESSION_A], connectSession: LIVEKIT_CONNECT });
    const backendB = aSessionServiceBackend({ sessions: [], connectSession: LIVEKIT_CONNECT });

    // When — select the live host-B session, open the inspector, and Terminate
    mountCrossHost(backendA, backendB, [SESSION_B_ACTIVE]);
    sessionsDrawerPage.drawerItem(SESSION_B_ACTIVE.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDetailsTab().click();
    sessionsDrawerPage.inspectorTerminateBtn(SESSION_B_ACTIVE.sessionId).click();

    // Then — SignalSession(SIGTERM) lands on host B, never host A
    cy.wrap(backendB).should((b: SessionServiceBackend) => {
      expect(b.signalCalls, "backendB (host B) SignalSession calls").to.have.length(1);
      expect(b.signalCalls[0].sessionId).to.equal(SESSION_B_ACTIVE.sessionId);
      expect(b.signalCalls[0].signal).to.equal(Signal.SIGTERM);
    });
    cy.wrap(backendA).should((b: SessionServiceBackend) => {
      expect(b.signalCalls, "backendA (host A) SignalSession calls").to.have.length(0);
    });
  });

  it("still lists a legacy session with an empty daemonInstanceId on a single daemon", () => {
    // Given — one daemon, one session the daemon returns with an empty daemonInstanceId (legacy
    // local daemon). Regression guard for the owning-host attribution fallback.
    const legacySession = { ...SESSION_A, daemonInstanceId: "" };
    const backend = aSessionServiceBackend({ sessions: [legacySession] });

    // When — mounted with the single default fixture daemon
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // Then — the session is listed (attributed to the selected daemon)
    sessionsDrawerPage.drawerItem(legacySession.sessionId).should("exist");
  });
});

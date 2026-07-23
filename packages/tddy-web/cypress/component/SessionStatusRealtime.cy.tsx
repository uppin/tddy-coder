/**
 * Behaviour spec: a session's status in the sessions list must track the daemon in real time —
 * when a session ends (its PID dies), its drawer row must flip from connected to disconnected on
 * its own, WITHOUT the operator reloading the page.
 *
 * Bug: the daemon computes a session's `isActive` from a live PID-liveness check
 * (see `session_reader.rs`); the frontend cannot be push-updated by the daemon. `SessionManager`
 * only refetches `ListSessions` on mount, on a manual `tddy-sessions-refresh` event, and on LiveKit
 * participant changes — there is no poll and no `WatchSessions` stream. So a session that ends by
 * itself (or from another client) keeps showing a green "connected" dot until the whole page is
 * reloaded. The operator sees a session as live long after it has actually ended.
 *
 * This test does NOT click Terminate — Terminate's own one-shot refetch is covered by
 * `SessionTerminateRefetch.cy.tsx`. Here the session ends *externally*, so the only thing that can
 * make the row update is the list keeping itself fresh on its own.
 *
 * The status dot's `data-status` is derived purely from `SessionEntry.isActive` via
 * `connectionStatusForSession`, so it is the authoritative, attachment-independent reflection of a
 * session's liveness in the list. The in-memory backend's `listSessionsFactory` is re-evaluated on
 * every `ListSessions` call, so it can report the session alive at mount and dead a moment later —
 * modelling the real PID-liveness flip.
 *
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`), routed over the shared common-room
 * LiveKit connection — see `aConnectionServiceBackend` (in-memory fake) and `withSelectedDaemon`.
 *
 * Feature: `docs/ft/web/session-drawer.md` (session connection state / real-time status).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend, type ConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LIVE_SESSION = {
  sessionId: "realtime-status-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-01T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/realtime-status-branch",
  pid: 30001,
  isActive: true,
  projectId: "proj-realtime-status-1",
  daemonInstanceId: "",
  workflowGoal: "Session that ends on its own while being watched",
  pendingElicitation: false,
};

/** How long after mount the session's process is reported dead, modelling an external end. */
const ENDS_AFTER_MS = 500;

/**
 * Mount the sessions drawer against a backend whose `ListSessions` reports `LIVE_SESSION` as active
 * until `ENDS_AFTER_MS` after mount, then inactive — as if its PID died with no user action.
 */
function aSessionThatEndsExternally(): ConnectionServiceBackend {
  const mountedAt = Date.now();
  const backend = aConnectionServiceBackend({
    listSessionsFactory: () => {
      const alive = Date.now() - mountedAt < ENDS_AFTER_MS;
      return [{ ...LIVE_SESSION, isActive: alive, status: alive ? "active" : "ended" }];
    },
  });
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SessionsDrawerScreen — session status tracks the daemon in real time", () => {
  it("flips a session's list row from connected to disconnected when it ends externally, without a reload", () => {
    // Given — a session that is live at mount, then ends on its own a moment later
    aSessionThatEndsExternally();

    // Then — the row starts out showing the session as connected
    sessionsDrawerPage
      .drawerItemStatus(LIVE_SESSION.sessionId)
      .should("have.attr", "data-status", "connected");

    // Then — once the daemon reports the session dead, the row updates itself to disconnected with
    // no page reload and no user action (this is what fails today: the list never refetches)
    sessionsDrawerPage
      .drawerItemStatus(LIVE_SESSION.sessionId, { timeout: 8000 })
      .should("have.attr", "data-status", "disconnected");
  });
});

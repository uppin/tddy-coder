/**
 * Acceptance tests: fast session change — re-selecting an already-attached session is an instant
 * focus switch with no RPC round-trip.
 *
 * The runtime registry keeps one mounted terminal per attached session, surviving focus switches,
 * and each `SessionRuntime` owns its own terminal-control lease (`useTerminalControl`). So selecting
 * a session whose runtime is already in the registry must restore the attachment from the registry
 * and re-focus it — it must NOT re-invoke `ConnectSession` (which would disrupt the live stream and
 * race a fresh `ClaimTerminalControl` against in-flight input, the original "terminal controlled by
 * another screen" / "not switching to the right session stream" bug).
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`.
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

const SESSION_A = {
  sessionId: "fast-a-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T09:00:00Z",
  status: "active" as const,
  repoPath: "/home/dev/feature-a",
  isActive: true,
  projectId: "proj-fast",
  workflowGoal: "Feature A",
};

const SESSION_B = {
  sessionId: "fast-b-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-12T09:05:00Z",
  status: "active" as const,
  repoPath: "/home/dev/feature-b",
  isActive: true,
  projectId: "proj-fast",
  workflowGoal: "Feature B",
};

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

/** A two-active-session drawer backend that records `ConnectSession` + `ClaimTerminalControl` calls
 *  so the fast-path can assert neither fires on re-select. Each attach resolves to a distinct
 *  LiveKit room so the two runtimes are distinguishable. */
function aFastSessionChangeBackend(): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [SESSION_A, SESSION_B],
    connectSession: (sessionId) => ({
      livekitRoom: sessionId === SESSION_A.sessionId ? "room-fast-a" : "room-fast-b",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server-fast",
    }),
  });
}

function aFastSessionChangeScreen() {
  const backend = aFastSessionChangeBackend();
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

const fastSessionChangePage = {
  select: (sessionId: string) => sessionsDrawerPage.drawerItem(sessionId).click(),
  expectFocused: (sessionId: string) =>
    sessionsDrawerPage.drawerItem(sessionId).should("have.attr", "aria-selected", "true"),
  expectAttachOpened: (sessionId: string) =>
    sessionsDrawerPage.runtimeTerminal(sessionId).should("exist"),
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("re-selecting an already-attached session does not re-invoke ConnectSession or ClaimTerminalControl", () => {
  // Given — a drawer with two active sessions, both attached
  const backend = aFastSessionChangeScreen();
  fastSessionChangePage.select(SESSION_A.sessionId);
  fastSessionChangePage.expectAttachOpened(SESSION_A.sessionId);
  fastSessionChangePage.select(SESSION_B.sessionId);
  fastSessionChangePage.expectAttachOpened(SESSION_B.sessionId);

  // each first-attach fires ConnectSession + ClaimTerminalControl exactly once, in selection order
  cy.wrap(backend.connectedSessionIds).should("deep.equal", [SESSION_A.sessionId, SESSION_B.sessionId]);
  cy.wrap(backend.claimedControlSessionIds).should("deep.equal", [SESSION_A.sessionId, SESSION_B.sessionId]);

  // When — re-select session A (already attached)
  fastSessionChangePage.select(SESSION_A.sessionId);

  // Then — focus switches back to A, but neither ConnectSession nor ClaimTerminalControl fires again
  fastSessionChangePage.expectFocused(SESSION_A.sessionId);
  cy.wrap(backend.connectedSessionIds).should("deep.equal", [SESSION_A.sessionId, SESSION_B.sessionId]);
  cy.wrap(backend.claimedControlSessionIds).should("deep.equal", [SESSION_A.sessionId, SESSION_B.sessionId]);
});

it("attaches a never-attached session via ConnectSession on first select (slow path still works)", () => {
  // Given — a freshly mounted drawer (no runtimes attached yet)
  const backend = aFastSessionChangeScreen();

  // When — select session A for the first time
  fastSessionChangePage.select(SESSION_A.sessionId);

  // Then — ConnectSession fires once for A and its runtime terminal mounts
  cy.wrap(backend.connectedSessionIds).should("deep.equal", [SESSION_A.sessionId]);
  fastSessionChangePage.expectAttachOpened(SESSION_A.sessionId);
});

/**
 * Acceptance test: selecting a session auto-focuses its terminal — no click required.
 *
 * Each attached session keeps one mounted terminal in the runtime registry, CSS-toggled between
 * focused (visible) and backgrounded (`display:none`). A backgrounded terminal keeps whatever DOM
 * focus state it had when it left the foreground, so bringing a session back to the foreground must
 * actively return keyboard focus to its terminal — otherwise the user has to click the terminal
 * before they can type into the session they just selected.
 *
 * Re-selecting an already-attached session is the case that pins this: its terminal is already
 * mounted (so there is no fresh mount-time auto-focus to lean on), and selection only flips CSS
 * visibility. Focus must follow the selection.
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
  sessionId: "focus-a-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T09:00:00Z",
  status: "active" as const,
  repoPath: "/home/dev/feature-a",
  isActive: true,
  projectId: "proj-focus",
  workflowGoal: "Feature A",
};

const SESSION_B = {
  sessionId: "focus-b-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-12T09:05:00Z",
  status: "active" as const,
  repoPath: "/home/dev/feature-b",
  isActive: true,
  projectId: "proj-focus",
  workflowGoal: "Feature B",
};

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

/** A two-active-session drawer backend; each attach resolves to a distinct LiveKit room so the two
 *  runtimes (and their terminals) are independently addressable. */
function aTerminalFocusBackend(): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [SESSION_A, SESSION_B],
    connectSession: (sessionId) => ({
      livekitRoom: sessionId === SESSION_A.sessionId ? "room-focus-a" : "room-focus-b",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server-focus",
    }),
  });
}

function aTerminalFocusScreen(): ConnectionServiceBackend {
  const backend = aTerminalFocusBackend();
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

const terminalFocusPage = {
  select: (sessionId: string) => sessionsDrawerPage.drawerItem(sessionId).click(),
  expectAttached: (sessionId: string) =>
    sessionsDrawerPage.runtimeTerminal(sessionId).should("exist"),
  expectTerminalFocused: (sessionId: string) =>
    sessionsDrawerPage.runtimeTerminalInput(sessionId).should("have.focus"),
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

it("re-selecting an already-attached session moves keyboard focus to its terminal without a click", () => {
  // Given — a drawer with two active sessions, both attached (A first, then B — so B holds focus)
  aTerminalFocusScreen();
  terminalFocusPage.select(SESSION_A.sessionId);
  terminalFocusPage.expectAttached(SESSION_A.sessionId);
  terminalFocusPage.select(SESSION_B.sessionId);
  terminalFocusPage.expectAttached(SESSION_B.sessionId);

  // When — the user switches back to session A (its terminal is already mounted)
  terminalFocusPage.select(SESSION_A.sessionId);

  // Then — session A's terminal receives keyboard focus, ready to type, with no click
  terminalFocusPage.expectTerminalFocused(SESSION_A.sessionId);
});

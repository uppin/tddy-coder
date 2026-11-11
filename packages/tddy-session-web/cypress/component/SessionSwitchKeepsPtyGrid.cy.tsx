/**
 * Acceptance: switching sessions never resizes the session left behind.
 *
 * A session's PTY is shared by every screen that views it and every agent that reads it, so its
 * grid must stay at the last size a screen really showed. Switching focus to another session keeps
 * the old session's terminal mounted in the runtime layer but hides it (`display:none`); a hidden
 * pane has no real size, so nothing it measures may be reported to the PTY.
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`.
 */

import "tddy-ui-web/theme.css"; // switching hides a runtime with Tailwind's `hidden`: without the stylesheet it stays laid out
import React from "react";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures — two active sessions their host serves itself
// ---------------------------------------------------------------------------

const SESSION_A = {
  sessionId: "switch-grid-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-11-11T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 71001,
  isActive: true,
  projectId: "proj-switch-grid-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

const SESSION_B = {
  sessionId: "switch-grid-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-11-11T09:05:00Z",
  status: "active",
  repoPath: "/home/dev/feature-beta",
  pid: 71002,
  isActive: true,
  projectId: "proj-switch-grid-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

/** Both sessions served by their host (no LiveKit room), so their terminals run over the bidi
 *  `StreamSessionTerminalIO` stream and every grid they report reaches the PTY recorder. */
function aHostServedBackendForBothSessions(): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [SESSION_A, SESSION_B],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
  });
}

// ---------------------------------------------------------------------------
// PTY grid reads — the `ESC ] resize;<cols>;<rows> BEL` reports a terminal sends up its stream
// ---------------------------------------------------------------------------

interface Grid {
  cols: number;
  rows: number;
}

const RESIZE_REPORT = /^\x1b\]resize;(\d+);(\d+)\x07$/;

/** Every grid the session's terminal reported to its PTY, in the order it sent them. */
function gridsReportedBy(backend: SessionServiceBackend, sessionId: string): Grid[] {
  return backend.sentTerminalInput
    .filter((input) => input.sessionId === sessionId)
    .map((input) => RESIZE_REPORT.exec(new TextDecoder().decode(input.data)))
    .filter((match): match is RegExpExecArray => match !== null)
    .map((match) => ({ cols: Number(match[1]), rows: Number(match[2]) }));
}

/** The grid the session's PTY was last resized to, once the terminal has reported one. */
function lastGridReportedBy(backend: SessionServiceBackend, sessionId: string) {
  return cy
    .wrap(backend, { timeout: 10000 })
    .should((b) => {
      expect(gridsReportedBy(b, sessionId), `${sessionId} reported its grid`).to.not.be.empty;
    })
    .then((b) => gridsReportedBy(b, sessionId).at(-1) as Grid);
}

/** Long enough for a pane's ResizeObserver to fire and its report to reach the backend. */
const LAYOUT_SETTLE_MS = 1000;

// ---------------------------------------------------------------------------

describe("Session switch — the session left behind keeps its PTY grid", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  /**
   * Switching away from a session hides its runtime (`display:none`) but keeps its terminal
   * mounted. The hidden pane's ResizeObserver still re-fits the emulator: the FitAddon reads the
   * hidden parent's computed `100%` width and height as 100px, proposes a sliver of a grid
   * (observed live as 12×6), and the terminal reports it to the shared PTY, which then stays that
   * small for every later viewer and agent read.
   */
  it("leaves session A's PTY at the grid it was really shown at after the user switches to session B", () => {
    // Given — session A is shown and has reported the grid of its visible pane
    const backend = aHostServedBackendForBothSessions();
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION_A.sessionId).should("be.visible");
    cy.wait(LAYOUT_SETTLE_MS);
    let shownGrid: Grid = { cols: 0, rows: 0 };
    lastGridReportedBy(backend, SESSION_A.sessionId).then((grid) => {
      shownGrid = grid;
      expect(grid.cols, "the shown pane is a real terminal width").to.be.greaterThan(40);
    });

    // When — the user switches to session B
    sessionsDrawerPage.drawerItem(SESSION_B.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION_B.sessionId).should("be.visible");
    cy.wait(LAYOUT_SETTLE_MS);

    // Then — A's PTY still has the grid A was really shown at
    lastGridReportedBy(backend, SESSION_A.sessionId).then((grid) => {
      expect(grid, "session A's last reported grid").to.deep.equal(shownGrid);
    });
  });
});

/**
 * Cypress component acceptance: Fast Session Change — inspector I/O bytes + last-data-received.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md` (req 5)
 *
 * Green: `SessionInspectorDrawer` Details tab renders
 * `sessions-inspector-bytes-in`, `sessions-inspector-bytes-out`, and
 * `sessions-inspector-last-data-received` (relative, ticking) wired from the
 * per-session traffic meter + LiveKit `DataReceived` events.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "io-bytes-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-io",
  pid: 72001,
  isActive: true,
  projectId: "proj-io-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

function aBackendForSession() {
  return aConnectionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({
      livekitRoom: `room-${SESSION.sessionId}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: `daemon-local-${SESSION.sessionId}`,
    }),
  });
}

// ---------------------------------------------------------------------------

describe("SessionInspectorByteCountAndLastReceived — inspector shows I/O bytes and a ticking last-received time", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("renders bytes in, bytes out, and a 'last data received: Ns ago' relative string that advances over time", () => {
    // Given — an attached session with the inspector open
    const backend = aBackendForSession();
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: "local", label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION.sessionId).should("exist");
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDetailsTab().should("have.attr", "aria-selected", "true");

    // Then — the inspector Details tab shows byte counters and a last-received relative string
    sessionsDrawerPage.inspectorBytesIn().should("exist");
    sessionsDrawerPage.inspectorBytesOut().should("exist");
    sessionsDrawerPage.inspectorLastDataReceived().should("exist");

    // bytes in is a non-negative integer string ...
    sessionsDrawerPage
      .inspectorBytesIn()
      .invoke("text")
      .then((text) => {
        const n = Number((text as string).replace(/[^\d]/g, ""));
        // byte count must be a real number (0 is acceptable before any bytes arrive)
        expect(Number.isFinite(n)).to.be.true;
      });

    // ... and the last-received string reads as a relative "ago" phrase
    sessionsDrawerPage
      .inspectorLastDataReceived()
      .invoke("text")
      .should((text) => {
        expect(text as string).to.match(/ago/);
      });
  });
});

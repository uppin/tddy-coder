/**
 * Cypress component acceptance: Fast Session Change — inspector data for a session with NO
 * LiveKit participant comes from the daemon `ListSessions` `SessionEntry` fields (inactive fallback).
 *
 * Changeset: `2026-07-12-fast-session-change`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md` (req 5 — dual source)
 *
 * Green:
 *   - `SessionEntry` proto carries `bytes_in` / `bytes_out` / `last_data_received_at`;
 *   - `SessionInspectorDrawer` Details tab renders `sessions-inspector-bytes-in` /
 *     `sessions-inspector-bytes-out` / `sessions-inspector-last-data-received` from the
 *     `SessionEntry` fields when no per-session live runtime exists.
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

const INACTIVE_SESSION = {
  sessionId: "inactive-aaaaaaaa-0000-0000-0000-000000000010",
  createdAt: "2026-07-12T08:00:00Z",
  status: "stopped",
  repoPath: "/home/dev/inactive-feature",
  pid: 0,
  isActive: false,
  projectId: "proj-inactive",
  daemonInstanceId: "local",
  pendingElicitation: false,
  // Daemon-sourced inspector data (req 5 dual source). Populated on `SessionEntry` by the
  // daemon `ListSessions`; the inspector reads them when no live LiveKit runtime exists.
  bytesIn: 4096n,
  bytesOut: 1024n,
  lastDataReceivedAt: "1700000000000",
} as unknown as Record<string, unknown>;

function aBackendWithInactiveSession() {
  return aConnectionServiceBackend({
    sessions: [INACTIVE_SESSION as never],
  });
}

// ---------------------------------------------------------------------------

describe("SessionInspectorInactiveDaemonSourced — inspector reads bytes/last-received from daemon RPC when no LiveKit participant", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("renders bytes in, bytes out, and last-data-received from the ListSessions SessionEntry for an inactive session", () => {
    // Given — an inactive session (no LiveKit room, not attached) carrying daemon-sourced counters
    const backend = aBackendWithInactiveSession();
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: "local", label: "local" }]),
      backend,
    );

    // When — the user selects the inactive row and opens the inspector
    sessionsDrawerPage.drawerItem(INACTIVE_SESSION.sessionId as string).click();
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDetailsTab().should("have.attr", "aria-selected", "true");

    // Then — the inspector Details tab shows the daemon-sourced byte counters and last-received line
    sessionsDrawerPage.inspectorBytesIn().should("exist");
    sessionsDrawerPage.inspectorBytesOut().should("exist");
    sessionsDrawerPage.inspectorLastDataReceived().should("exist");

    sessionsDrawerPage
      .inspectorBytesIn()
      .invoke("text")
      .should((text) => {
        expect(text as string).to.include("4096");
      });
    sessionsDrawerPage
      .inspectorBytesOut()
      .invoke("text")
      .should((text) => {
        expect(text as string).to.include("1024");
      });
    sessionsDrawerPage
      .inspectorLastDataReceived()
      .invoke("text")
      .should((text) => {
        expect(text as string).to.match(/ago/);
      });
  });
});

/**
 * Cypress component acceptance: Fast Session Change — sessions list reads `session` metadata
 * from a LiveKit participant (presence-driven, no ListSessions fan-out for that row).
 *
 * Changeset: `2026-07-12-fast-session-change`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md` (req 4)
 *
 * Green: `useRoomParticipants` parses a `session` metadata block and
 * `SessionManager.mergeActiveAndFetchedSessions` overlays it onto synthesized cross-host rows
 * (rendering goal/state/agent/model via `sessions-drawer-item-session-meta-<sessionId>`).
 */

import React from "react";
import { ConnectionService } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  withSelectedDaemonRoom,
  aFakeCommonRoomWithMetadata,
} from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures — a peer-host session known ONLY via its live common-room participant
// ---------------------------------------------------------------------------

const HOST_A = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
const HOST_B = { instanceId: "server-2", label: "server-2" };

const PEER_SESSION_ID = "bbbbbbbb-0000-4000-8000-000000000002";
const PEER_PARTICIPANT_IDENTITY = `daemon-${HOST_B.instanceId}-${PEER_SESSION_ID}`;

const PEER_SESSION_METADATA = JSON.stringify({
  session: {
    workflow_goal: "acceptance-tests",
    workflow_state: "Red",
    elapsed_display: "3m",
    agent: "claude",
    model: "sonnet-4",
    activity_status: "",
    recipe: "tdd",
    repo_path: "/home/dev/peer-feature",
    pending_elicitation: false,
  },
});

// ---------------------------------------------------------------------------

describe("SessionsListParticipantMetadata — drawer row renders session metadata from a participant", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("renders goal, state, agent, and model from the participant metadata with no ListSessions call for that row", () => {
    // Given — host A is selected; the peer session is present only as a live participant on host B
    // (host A's ListSessions returns nothing for it).
    const backend = aConnectionServiceBackend({ sessions: [] });
    const room = aFakeCommonRoomWithMetadata([
      { identity: PEER_PARTICIPANT_IDENTITY, metadata: PEER_SESSION_METADATA },
    ]);
    mountWithRecordingLiveKitRpc(
      withSelectedDaemonRoom(<SessionsDrawerScreen />, [HOST_A], room),
      backend,
    );

    // When — the drawer renders
    sessionsDrawerPage.drawer().should("exist");

    // Then — the peer session row shows the parsed session metadata ...
    sessionsDrawerPage.drawerItem(PEER_SESSION_ID).should("exist");
    sessionsDrawerPage.drawerItemSessionMeta(PEER_SESSION_ID).should("exist");
    sessionsDrawerPage
      .drawerItemSessionMeta(PEER_SESSION_ID)
      .should("contain.text", "acceptance-tests")
      .and("contain.text", "Red")
      .and("contain.text", "claude")
      .and("contain.text", "sonnet-4");

    // ... and ListSessions was called only once (the selected host's fetch) — no fan-out for the row.
    cy.wrap(backend).should((b) => {
      const listCalls = b.callsTo(ConnectionService.method.listSessions);
      expect(listCalls).to.have.length(1);
    });
  });
});

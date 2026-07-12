/**
 * Cypress component acceptance: Fast Session Change — session-scoped RPCs route through
 * the session's room participant, not the daemon participant.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md` (req 1)
 *
 * Green: `SessionsDrawerScreen` builds the session-scoped
 * `ConnectionService` client via `liveKitFactory(room, "daemon-<instanceId>-<sessionId>")`
 * for `ExecuteTool` / `ClaimTerminalControl`. `DeleteSession` / `SignalSession` stay on the
 * daemon participant (`daemon-<instanceId>`) — see SessionStartResumeStillDaemonRouted.cy.tsx.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures — one active session on the local daemon
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "routing-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-routing",
  pid: 71001,
  isActive: true,
  projectId: "proj-route-1",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

const DAEMON_INSTANCE_ID = "local";
/** The session participant identity the session-scoped RPCs must target. */
const SESSION_PARTICIPANT_IDENTITY = `daemon-${DAEMON_INSTANCE_ID}-${SESSION.sessionId}`;
/** The daemon participant identity that must NOT receive session-scoped RPCs. */
const DAEMON_PARTICIPANT_IDENTITY = `daemon-${DAEMON_INSTANCE_ID}`;

/** An inactive session — the inspector's Delete control only renders for inactive sessions, and
 *  selecting one auto-opens the inspector. Used by the DeleteSession-routing sub-test. */
const INACTIVE_SESSION = {
  ...SESSION,
  sessionId: "routing-bbbbbbbb-0000-0000-0000-000000000002",
  isActive: false,
  status: "stopped",
};
const INACTIVE_SESSION_PARTICIPANT_IDENTITY = `daemon-${DAEMON_INSTANCE_ID}-${INACTIVE_SESSION.sessionId}`;

function aBackendForSession() {
  return aConnectionServiceBackend({
    sessions: [SESSION],
    connectSession: () => ({
      livekitRoom: `room-${SESSION.sessionId}`,
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: SESSION_PARTICIPANT_IDENTITY,
    }),
  });
}

// ---------------------------------------------------------------------------

describe("SessionParticipantRpcRouting — session-scoped RPCs target the session participant", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("routes ExecuteTool to the session participant identity, not the daemon participant", () => {
    // Given — an attached session
    const backend = aBackendForSession();
    const harness = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: DAEMON_INSTANCE_ID, label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION.sessionId).should("exist");

    // When — the user invokes a tool from the inspector Tools tab
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorToolsTab().click();
    byTestId(TEST_IDS.sessionsToolInvokeButton).click();

    // Then — ExecuteTool reached the backend and a LiveKit client was built for the session participant
    cy.wrap(backend).should((b) => {
      expect(b.executedToolSessionIds).to.deep.equal([SESSION.sessionId]);
    });
    cy.wrap(harness).should((h) => {
      expect(h.targets).to.include(SESSION_PARTICIPANT_IDENTITY);
    });
  });

  it("does NOT route DeleteSession to the session participant (delete stays daemon-direct)", () => {
    // Given — an inactive session (the inspector's Delete control only renders for inactive
    // sessions; selecting one auto-opens the inspector). Delete must still route daemon-direct so
    // it works even when the coder participant is stuck.
    const backend = aConnectionServiceBackend({ sessions: [INACTIVE_SESSION] });
    const harness = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: DAEMON_INSTANCE_ID, label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(INACTIVE_SESSION.sessionId).click();
    // Inspector auto-opens for inactive sessions on select — no toggle needed.
    sessionsDrawerPage.inspectorDeleteBtn(INACTIVE_SESSION.sessionId).click();
    sessionsDrawerPage.inspectorDeleteConfirm(INACTIVE_SESSION.sessionId).click();

    // Then — DeleteSession reached the backend via the DAEMON participant identity (delete is
    // daemon-direct so it still works when the coder participant is stuck), NOT the session
    // participant identity.
    cy.wrap(backend).should((b) => {
      expect(b.deletedSessionIds).to.deep.equal([INACTIVE_SESSION.sessionId]);
    });
    cy.wrap(harness).should((h) => {
      expect(h.targets, "DeleteSession must target the daemon participant").to.include(
        DAEMON_PARTICIPANT_IDENTITY,
      );
      expect(
        h.targets,
        "DeleteSession must NOT target the session participant",
      ).not.to.include(INACTIVE_SESSION_PARTICIPANT_IDENTITY);
    });
  });

  it("routes ClaimTerminalControl to the session participant identity when the claim button is clicked", () => {
    // Given — an attached session whose terminal control is held elsewhere (overlay visible)
    const backend = aBackendForSession();
    const harness = mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [{ instanceId: DAEMON_INSTANCE_ID, label: "local" }]),
      backend,
    );
    sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
    sessionsDrawerPage.runtimeTerminal(SESSION.sessionId).should("exist");

    // When — the user clicks "Claim terminal"
    sessionsDrawerPage.terminalClaimBtn().click();

    // Then — ClaimTerminalControl reached the backend via the session participant identity.
    // The auto-claim-on-attach (steal=false, daemon-direct) and the explicit "Claim terminal"
    // button (steal=true, session-participant) both fire — both must target this session. The
    // session-participant routing is asserted via `h.targets` below.
    cy.wrap(backend).should((b) => {
      expect(b.claimedControlSessionIds).to.have.length(2);
      expect(b.claimedControlSessionIds.every((id) => id === SESSION.sessionId)).to.be.true;
    });
    cy.wrap(harness).should((h) => {
      expect(h.targets).to.include(SESSION_PARTICIPANT_IDENTITY);
    });
  });
});

/**
 * Acceptance: a **live session whose terminal feed drops gets its terminal back on its own**.
 *
 * Reproduces a production incident (2026-08-13, host `udoo`, session `019fc153-…`). The operator
 * resumed a dormant session; the daemon spawned it and served the terminal, then the output stream
 * ended. `GrpcSessionTerminal` reports that as a disconnect, and `SessionsDrawerScreen`'s
 * `onSessionDisconnect` evicts the runtime and resets the attachment — but keeps the *attach claim*
 * it took for this session. The liveness effect returns early on that claim, so nothing ever
 * re-attached: the pane fell through to the "Select Resume to reconnect" placeholder for a session
 * the daemon was reporting alive, with no Resume button in the top bar (Resume is keyed on
 * dormancy). Leaving the session and coming back re-attached it; nothing else did.
 *
 * The rule these tests pin: an evicted runtime on a live session is re-attached without the operator
 * doing anything, and the recovery is an *attach* — a live agent is never resumed a second time.
 *
 * Driven over the gRPC terminal path (`livekitRoom: ""`), which is what the incident session used:
 * its `.session.yaml` carried no `livekit_room`, so every terminal RPC flowed over the daemon client.
 */

import React from "react";
import { ConnectionService } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { replayAgentText } from "../support/rpc/acpReplay";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const DORMANT = {
  sessionId: "feed-drop-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-08-02T07:15:32Z",
  status: "exited",
  repoPath: "/var/tddy/repos/tddy-coder/.worktrees/feat-supervisor",
  pid: 0,
  isActive: false,
  projectId: "proj-feed-drop",
  daemonInstanceId: "local",
  workflowGoal: "Supervise the worker pool",
  pendingElicitation: false,
};

const LIVE = {
  ...DORMANT,
  sessionId: "feed-drop-bbbbbbbb-0000-0000-0000-000000000002",
  status: "active",
  pid: 44861,
  isActive: true,
};

/** The transcript the daemon replays while the session is dormant — the Activities view's content. */
const RECORDED_TRANSCRIPT = {
  counts: [1],
  snapshot: [replayAgentText("Spawned the supervisor.", 1_000)],
};

/** A terminal attach that streams over the daemon client rather than a per-session LiveKit room. */
const OVER_GRPC = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

// ---------------------------------------------------------------------------
// Mount + synchronisation helpers
// ---------------------------------------------------------------------------

function mountScreen(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
}

/**
 * Waits until the daemon has served `count` terminal feeds for the session. Re-opening a feed is the
 * screen's own evidence that it re-attached after the drop, so this is the synchronisation point a
 * recovery assertion needs — without it, "the terminal exists" could pass against the *first* attach,
 * before the feed had dropped at all.
 *
 * 10s: the recovery is gated on the screen's 2s session-list poll, so a 4s default can expire on a
 * loaded runner before the first post-eviction poll has even landed.
 */
function awaitTerminalFeedsServed(backend: ConnectionServiceBackend, count: number) {
  cy.wrap(null, { timeout: 10_000 }).should(() => {
    expect(backend.streamedTerminals, "terminal feeds opened").to.have.length(count);
  });
}

// ---------------------------------------------------------------------------

describe("SessionTerminalFeedRecovery — a live session re-attaches after its feed drops", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows the terminal again after the resumed session's terminal feed drops", () => {
    // Given — a dormant session the daemon reports alive once resumed, whose first terminal feed
    // ends the way the incident's did
    let sessionIsLive = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [
        sessionIsLive ? { ...DORMANT, isActive: true, status: "active", pid: 44861 } : DORMANT,
      ],
      acpReplay: RECORDED_TRANSCRIPT,
      resumeSession: OVER_GRPC,
      connectSession: OVER_GRPC,
      droppedTerminalStreams: 1,
    });

    // When — the operator resumes it from the top bar and the daemon reports it alive, and the
    // terminal feed it was given drops. The operator does nothing else: no re-selection, no reload.
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();
    sessionActivitiesPage.resume(DORMANT.sessionId);
    cy.then(() => {
      sessionIsLive = true;
    });
    awaitTerminalFeedsServed(backend, 2);

    // Then — the terminal owns the pane again, rather than the reconnect placeholder
    page.detailTerminalContainer().should("exist");
  });

  it("recovers a dropped feed by re-attaching, never by resuming the live agent again", () => {
    // Given — the same resumed session whose first terminal feed drops
    let sessionIsLive = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [
        sessionIsLive ? { ...DORMANT, isActive: true, status: "active", pid: 44861 } : DORMANT,
      ],
      acpReplay: RECORDED_TRANSCRIPT,
      resumeSession: OVER_GRPC,
      connectSession: OVER_GRPC,
      droppedTerminalStreams: 1,
    });

    // When — it is resumed once and the feed drops
    mountScreen(backend);
    page.drawerItem(DORMANT.sessionId).click();
    sessionActivitiesPage.resume(DORMANT.sessionId);
    cy.then(() => {
      sessionIsLive = true;
    });
    awaitTerminalFeedsServed(backend, 2);

    // Then — the recovery took one ConnectSession. The agent is already running, so a second
    // ResumeSession would spawn over a live process rather than re-attach to it.
    cy.wrap(null).should(() => {
      const resumed = backend
        .callsTo(ConnectionService.method.resumeSession)
        .map((c) => c.sessionId);
      expect(resumed, "ResumeSession calls").to.deep.equal([DORMANT.sessionId]);
      expect(backend.connectedSessionIds, "ConnectSession calls").to.deep.equal([
        DORMANT.sessionId,
      ]);
    });
  });

  it("shows the terminal again after a selected live session's terminal feed drops", () => {
    // Given — a session that was already alive when the operator selected it, so its attach came
    // from the selection rather than from a resume, and whose first terminal feed drops
    const backend = aConnectionServiceBackend({
      sessions: [LIVE],
      acpReplay: RECORDED_TRANSCRIPT,
      connectSession: OVER_GRPC,
      droppedTerminalStreams: 1,
    });

    // When — the operator selects it and the feed drops, with the selection never changing
    mountScreen(backend);
    page.drawerItem(LIVE.sessionId).click();
    awaitTerminalFeedsServed(backend, 2);

    // Then — the terminal is back, and the second feed came from a second attach
    page.detailTerminalContainer().should("exist");
    cy.wrap(null).should(() => {
      expect(backend.connectedSessionIds, "ConnectSession calls").to.deep.equal([
        LIVE.sessionId,
        LIVE.sessionId,
      ]);
    });
  });
});

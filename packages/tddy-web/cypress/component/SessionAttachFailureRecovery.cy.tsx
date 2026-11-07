/**
 * A live session whose first `ConnectSession` fails must not be left on the "Select Resume to
 * reconnect" placeholder.
 *
 * `useSessionAttachment.attach()` swallows the failure into `{ status: "error" }`, so the `.catch`
 * in `SessionsDrawerScreen` that is meant to give the attach claim back never runs. The claim stays
 * held, every later session-list poll answers `hold` for the live session, and nothing attaches it
 * again — while a live session offers no Resume button to recover by.
 *
 * Driven over the gRPC terminal path (`livekitRoom: ""`), so a successful attach needs no LiveKit.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

const LIVE = {
  sessionId: "attach-fail-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-10-17T15:20:00Z",
  status: "active",
  repoPath: "/var/tddy/repos/tddy-coder",
  pid: 44861,
  isActive: true,
  projectId: "proj-attach-fail",
  daemonInstanceId: "local",
  workflowGoal: "Serve the terminal",
  pendingElicitation: false,
};

/** A terminal attach that streams over the daemon client rather than a per-session LiveKit room. */
const OVER_GRPC = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

describe("SessionAttachFailureRecovery — a failed attach does not strand a live session", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("attaches the selected live session again after its first ConnectSession fails", () => {
    // Given — a live session whose first ConnectSession fails and whose second succeeds
    const backend = aSessionServiceBackend({
      sessions: [LIVE],
      connectSession: OVER_GRPC,
      failedConnectSessions: 1,
    });

    // When — the operator selects it and does nothing else
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(LIVE.sessionId).click();

    // Then — the terminal owns the pane, reached by a second attach. 10s: the retry is gated on
    // the screen's 2s session-list poll.
    cy.wrap(null, { timeout: 10_000 }).should(() => {
      expect(backend.connectedSessionIds, "ConnectSession calls").to.deep.equal([
        LIVE.sessionId,
        LIVE.sessionId,
      ]);
    });
    page.detailTerminalContainer().should("exist");
  });
});

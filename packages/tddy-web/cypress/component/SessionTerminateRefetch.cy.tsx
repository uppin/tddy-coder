/**
 * Behaviour spec: clicking "Terminate" in the session inspector must refetch the
 * session list, even when the underlying `SignalSession` RPC fails.
 *
 * Bug: the daemon computes a session's `isActive` from a live PID-liveness check
 * (see `session_reader.rs`), not from a value the frontend can push-update. If the
 * process already exited (e.g. it ended on its own, or a previous Terminate click
 * already succeeded) by the time a "Terminate" click reaches the daemon,
 * `SignalSession` fails with "process is not alive" — and until now
 * `SessionsDrawerScreen`'s `handleTerminate` only logged that error to
 * `console.debug`, never refetching the list. The row kept showing `isActive: true`
 * and the "Terminate" button never went away, making a session that had, in fact,
 * already ended look like clicking Terminate "did nothing".
 *
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`), routed over the shared
 * common-room LiveKit connection — see `aConnectionServiceBackend` (in-memory fake) and
 * `SelectedDaemonProvider` (via `withSelectedDaemon`).
 */

import React from "react";
import { ConnectError, Code } from "@connectrpc/connect";
import { ConnectionService, Signal } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend, type ConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CONNECTED_SESSION = {
  sessionId: "terminate-refetch-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-01T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/terminate-refetch-branch",
  pid: 20001,
  isActive: true,
  projectId: "proj-terminate-refetch-1",
  daemonInstanceId: "",
  workflowGoal: "Session that Terminate should end",
  pendingElicitation: false,
};

function mountAndSelect(backend: ConnectionServiceBackend) {
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CONNECTED_SESSION.sessionId).click();
  sessionsDrawerPage.inspectorToggle().click();
  sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("SessionsDrawerScreen — Terminate refetches the session list", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("refetches the session list after a successful Terminate", () => {
    // Given — the session starts active; ListSessions reports it inactive once SignalSession
    // has actually been received by the daemon.
    let terminated = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [{ ...CONNECTED_SESSION, isActive: !terminated }],
      connectSession: { livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    }).onUnary(ConnectionService.method.signalSession, async () => {
      // Flip synchronously inside the request handler — exactly when the app's own request
      // actually lands — so the *next* ListSessions response is guaranteed to reflect it.
      terminated = true;
      return {};
    });
    mountAndSelect(backend);

    // When — Terminate is clicked and SignalSession succeeds
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).click();
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.signalSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionId).to.equal(CONNECTED_SESSION.sessionId);
      expect(calls[0].signal).to.equal(Signal.SIGTERM);
    });

    // Then — the list is refetched, and the now-inactive row no longer offers Terminate
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).should("not.exist");
  });

  it("refetches the session list after Terminate, even when SignalSession fails because the session already ended", () => {
    // Given — the daemon still reports the session as active on the first load, but will report
    // it as inactive once SignalSession lands (simulating the live PID-liveness check flipping
    // once the process is confirmed dead — the same daemon-side check that makes SignalSession
    // itself fail with "process is not alive").
    let sessionEnded = false;
    const backend = aConnectionServiceBackend({
      listSessionsFactory: () => [{ ...CONNECTED_SESSION, isActive: !sessionEnded }],
      connectSession: { livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" },
    }).onUnary(ConnectionService.method.signalSession, async () => {
      sessionEnded = true;
      throw new ConnectError("Process not alive", Code.FailedPrecondition);
    });
    mountAndSelect(backend);

    // When — Terminate is clicked but SignalSession fails ("process is not alive")
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).click();
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ConnectionService.method.signalSession)).to.have.length(1);
    });

    // Then — the row now reflects the session as inactive, so Terminate is gone
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).should("not.exist");
  });
});

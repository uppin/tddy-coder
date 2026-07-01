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
 */

import React from "react";
import { fromBinary } from "@bufbuild/protobuf";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { SignalSessionRequestSchema, Signal } from "../../src/gen/connection_pb";
import { interceptConnectionRpcs, interceptConnectSession } from "../support/rpc/connectionRpcs";
import { decodeProtoRequestBody, toArrayBuffer } from "../support/rpc/protoRpc";
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

const OK_SIGNAL_SESSION_BODY = toArrayBuffer(new Uint8Array());

/**
 * Intercepts SignalSession and flips `onSignalReceived` synchronously inside the request
 * handler — i.e. exactly when the app's own request actually lands, not on a separate,
 * loosely-synchronized `cy.wait().then()` in the test. `listSessionsFactory` closures read
 * this flag, so the *next* ListSessions response is guaranteed to reflect the signal having
 * been sent, with no race between the test's bookkeeping and the app's own refetch.
 */
function interceptSignalSessionAnd(succeeds: boolean, onSignalReceived: () => void): void {
  const alias = succeeds ? "signalSession" : "signalSessionError";
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
    onSignalReceived();
    if (succeeds) {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: OK_SIGNAL_SESSION_BODY });
    } else {
      req.reply({
        statusCode: 412,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code: "failed_precondition", message: "Process not alive" }),
      });
    }
  }).as(alias);
}

function mountAndSelect() {
  cy.mount(<SessionsDrawerScreen />);
  cy.wait("@listSessions");
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
    interceptConnectionRpcs([CONNECTED_SESSION], {
      listSessionsFactory: () => [{ ...CONNECTED_SESSION, isActive: !terminated }],
    });
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });
    interceptSignalSessionAnd(true, () => {
      terminated = true;
    });
    mountAndSelect();

    // When — Terminate is clicked and SignalSession succeeds
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).click();
    cy.wait("@signalSession").then((interception) => {
      const decoded = fromBinary(SignalSessionRequestSchema, decodeProtoRequestBody(interception.request.body));
      expect(decoded.sessionId).to.equal(CONNECTED_SESSION.sessionId);
      expect(decoded.signal).to.equal(Signal.SIGTERM);
    });

    // Then — the list is refetched, and the now-inactive row no longer offers Terminate
    cy.wait("@listSessions");
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).should("not.exist");
  });

  it("refetches the session list after Terminate, even when SignalSession fails because the session already ended", () => {
    // Given — the daemon still reports the session as active on the first load, but will report
    // it as inactive once SignalSession lands (simulating the live PID-liveness check flipping
    // once the process is confirmed dead — the same daemon-side check that makes SignalSession
    // itself fail with "process is not alive").
    let sessionEnded = false;
    interceptConnectionRpcs([CONNECTED_SESSION], {
      listSessionsFactory: () => [{ ...CONNECTED_SESSION, isActive: !sessionEnded }],
    });
    interceptConnectSession({ livekitRoom: "room-a", livekitUrl: "ws://127.0.0.1:7880", livekitServerIdentity: "server" });
    interceptSignalSessionAnd(false, () => {
      sessionEnded = true;
    });
    mountAndSelect();

    // When — Terminate is clicked but SignalSession fails ("process is not alive")
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).click();
    cy.wait("@signalSessionError");

    // Then — the list is refetched despite the failure
    cy.wait("@listSessions");

    // Then — the row now reflects the session as inactive, so Terminate is gone
    sessionsDrawerPage.inspectorTerminateBtn(CONNECTED_SESSION.sessionId).should("not.exist");
  });
});

/**
 * A selected host that is not on the common room yet is waiting, and nothing is asked of it. After
 * a reload onto a non-default host the selector shows it waiting for a moment; a request sent to
 * its RPC participant before it has joined has nobody to answer it, so the screen records a failed
 * read and its sessions never appear once the host does.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React from "react";
import type { DaemonHost } from "../../src/lib/participantRole";
import { daemonRpcIdentity } from "../../src/lib/participantRole";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { appLocationPage } from "../support/pages/appLocationPage";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import {
  mountWithRecordingLiveKitRpc,
  type RecordedRpcCall,
} from "../support/rpc/recordingLiveKitRpc";
import { aJoinedCommonRoom, withSelectedDaemonRoom } from "../support/rpc/withSelectedDaemon";

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B = "laptop-b";

/** How long the waiting screen is given to send something it should not. */
const SETTLE_MS = 500;

function expectNoRpcSentTo(instanceId: string, calls: RecordedRpcCall[]) {
  const sentToIt = calls.filter((call) => call.targetIdentity === daemonRpcIdentity(instanceId));
  expect(sentToIt.map((call) => call.method)).to.deep.equal([]);
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  appLocationPage.reset();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("sends no RPC to the selected host while the common room has not listed it", () => {
  // Given — a reload onto laptop-b: the room is joined but only udoo is on it
  appLocationPage.startAt(`/sessions?host=${LAPTOP_B}`);
  const backend = aSessionServiceBackend({ sessions: [] });

  // When
  const { rpcCalls } = mountWithRecordingLiveKitRpc(
    withSelectedDaemonRoom(
      <SessionsDrawerScreen />,
      [UDOO],
      aJoinedCommonRoom([daemonRpcIdentity("udoo")]),
    ),
    backend,
  );
  sessionsDrawerPage.screen().should("be.visible");
  cy.wait(SETTLE_MS);

  // Then
  cy.wrap(rpcCalls).should((calls: RecordedRpcCall[]) => expectNoRpcSentTo(LAPTOP_B, calls));
});

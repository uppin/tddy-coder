/**
 * Acceptance test: the LiveKit screen (#/livekit) renders the "Connected participants" panel
 * extracted from the old ConnectionScreen, listing the shared common-room participants.
 *
 * PRD: docs/ft/web/app-shell.md § LiveKit screen
 */

import React from "react";
import { LiveKitAppPage } from "../../src/components/livekit/LiveKitAppPage";
import {
  DEFAULT_TEST_DAEMON,
  aFakeCommonRoom,
  withSelectedDaemonRoom,
} from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { byTestId, TEST_IDS, participantEntry } from "../support/testIds";

describe("LiveKit screen — connected participants", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("lists the common-room participants in the connected-participants panel", () => {
    // Given — a common room with two participants
    const room = aFakeCommonRoom(["browser-alice", "daemon-local"]);
    const backend = aConnectionServiceBackend();

    // When
    mountWithRecordingLiveKitRpc(
      withSelectedDaemonRoom(<LiveKitAppPage onNavigate={cy.stub()} />, [DEFAULT_TEST_DAEMON], room),
      backend,
    );

    // Then — the participants panel shows both participants
    byTestId(TEST_IDS.connectedParticipantsPanel).should("be.visible");
    byTestId(participantEntry("browser-alice")).should("exist");
    byTestId(participantEntry("daemon-local")).should("exist");
  });
});

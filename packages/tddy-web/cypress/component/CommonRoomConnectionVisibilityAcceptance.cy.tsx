/**
 * Acceptance tests for making a failed common-room join *visible* to the operator.
 *
 * Reproduces a production incident (2026-08-13, host `udoo`): a browser on a different subnet
 * opened the LiveKit signal WebSocket fine, but ICE never established, so `Room.connect()` rejected
 * ~14s later. `useCommonRoom` recorded that correctly as `status: "error"` with LiveKit's reason —
 * and then `SelectedDaemonProvider` dropped it on the floor, keeping only `room`, which is `null`
 * on failure. Downstream, `useObservedCommonRoomStatus(null)` reports `"idle"`, so the presence
 * panel showed "Connecting to presence room…" forever and the daemon selector sat empty and
 * disabled. Every symptom the operator could see was indistinguishable from "still connecting",
 * and the actual error existed only in `useCommonRoom`'s discarded state.
 *
 * These tests pin the connection outcome as something the provider *publishes*: the reason reaches
 * the presence panel, the daemon selector says why it has nothing to offer, and any daemon-mode
 * consumer can read the common room's own status and reason — off its host-directory source, since
 * the room itself is no longer on the shared context.
 */

import React from "react";
import { LiveKitAppPage } from "../../src/components/livekit/LiveKitAppPage";
import { LIVEKIT_SOURCE_ID } from "../../src/rpc/hostDirectory/liveKitSource";
import { useHostDirectorySource } from "../../src/rpc/hostDirectory/useHostDirectory";
import {
  aCommonRoomThatFailsToConnect,
  aCommonRoomThatNeverFinishesConnecting,
} from "../support/livekit/commonRoomConnection";
import { mountWithLiveCommonRoom } from "../support/rpc/withLiveCommonRoom";
import { participantListPage } from "../support/pages/participantListPage";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { byTestId } from "../support/testIds";

/** Verbatim livekit-client message when the peer connection never establishes (blocked ICE). */
const ICE_FAILURE = "could not establish pc connection";

// ---------------------------------------------------------------------------
// Test harness probe
// ---------------------------------------------------------------------------

/** Renders the common-room connection state the provider publishes to daemon-mode consumers. */
function CommonRoomStateProbe() {
  const commonRoom = useHostDirectorySource(LIVEKIT_SOURCE_ID);
  return (
    <div>
      <span data-testid="probe-room-status">{commonRoom?.status ?? "absent"}</span>
      <span data-testid="probe-room-error">{commonRoom?.error ?? "none"}</span>
    </div>
  );
}

const commonRoomStateProbe = {
  status: () => byTestId("probe-room-status", { timeout: 5000 }),
  error: () => byTestId("probe-room-error", { timeout: 5000 }),
};

// ---------------------------------------------------------------------------

describe("common-room connection visibility", () => {
  beforeEach(() => {
    cy.viewport(1200, 800);
    cy.clearLocalStorage();
  });

  it("names the reason the presence room could not be joined", () => {
    // Given — a common room that rejects the join, as LiveKit does when ICE cannot establish
    const unreachableRoom = aCommonRoomThatFailsToConnect(ICE_FAILURE);

    // When — the operator opens the LiveKit presence screen
    mountWithLiveCommonRoom(<LiveKitAppPage onNavigate={cy.stub()} />, unreachableRoom);

    // Then — the panel quotes the failure instead of claiming it is still connecting
    participantListPage.expectConnectionFailure(ICE_FAILURE);
  });

  it("tells the daemon selector the room is unreachable rather than showing an empty list", () => {
    // Given — the same unreachable common room, on a page no daemon named itself to, so the room
    // is the only thing that could contribute a host. A page served by a daemon offers that daemon
    // whatever the room does (see `HostDirectoryAcceptance`), and so is never empty to explain.
    const unreachableRoom = aCommonRoomThatFailsToConnect(ICE_FAILURE);

    // When — the operator opens a daemon-mode screen
    mountWithLiveCommonRoom(<LiveKitAppPage onNavigate={cy.stub()} />, unreachableRoom, {
      servedByADaemon: false,
    });

    // Then — the selector distinguishes "cannot reach the room" from "no daemons are running"
    daemonSelectorPage.expectUnreachable();
  });

  it("publishes the failed connection status and reason to daemon-mode consumers", () => {
    // Given — a common room that rejects the join
    const unreachableRoom = aCommonRoomThatFailsToConnect(ICE_FAILURE);

    // When — a consumer reads the daemon-selection context
    mountWithLiveCommonRoom(<CommonRoomStateProbe />, unreachableRoom);

    // Then — it sees the error, so any screen can explain itself without joining the room again
    commonRoomStateProbe.status().should("have.text", "error");
    commonRoomStateProbe.error().should("have.text", ICE_FAILURE);
  });

  it("publishes a connecting status while the join is still in flight", () => {
    // Given — a common room whose join has not settled yet
    const joiningRoom = aCommonRoomThatNeverFinishesConnecting();

    // When — a consumer reads the daemon-selection context
    mountWithLiveCommonRoom(<CommonRoomStateProbe />, joiningRoom);

    // Then — it sees an in-progress join, not a failure: the slow path must stay quiet
    commonRoomStateProbe.status().should("have.text", "connecting");
    commonRoomStateProbe.error().should("have.text", "none");
  });
});

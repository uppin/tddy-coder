/**
 * Driver for ConnectionScreen + SelectedDaemonProvider common-room wiring tests.
 *
 * Encapsulates production-like mount (no `room` override on the provider) and assertions
 * on how many times the browser joins the shared LiveKit lobby.
 */

import React from "react";
import { TokenService } from "../../../src/gen/token_pb";
import { AuthProvider } from "../../../src/hooks/authProvider";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider, useSelectedDaemon } from "../../../src/rpc/selectedDaemon";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
  type ConnectionServiceScenario,
} from "../rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../rpc/recordingLiveKitRpc";

const LIVEKIT_URL = "ws://127.0.0.1:7880";
const COMMON_ROOM = "tddy-lobby";

const DEV_DAEMON: DaemonHost = { instanceId: "dev", label: "dev (this daemon)" };

/**
 * Mirrors ConnectionScreen presence wiring: reuse `SelectedDaemonProvider`'s room, no second join.
 */
export function ConnectionScreenPresenceProbe() {
  useSelectedDaemon();
  return <div data-testid="connection-screen-presence-probe">presence probe</div>;
}

/** Mount the production common-room path: provider + ConnectionScreen-style presence join. */
export function mountConnectionScreenWithProductionCommonRoom(
  scenario: ConnectionServiceScenario = {},
): ConnectionServiceBackend {
  window.localStorage.setItem("tddy_session_token", "fake-token");
  const backend = aConnectionServiceBackend(scenario);
  mountWithRecordingLiveKitRpc(
    <AuthProvider>
      <SelectedDaemonProvider
        livekitUrl={LIVEKIT_URL}
        commonRoom={COMMON_ROOM}
        servingInstanceId="dev"
        daemons={[DEV_DAEMON]}
      >
        <ConnectionScreenPresenceProbe />
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
  return backend;
}

export const sharedCommonRoomPage = {
  /** Assert how many `TokenService.generateToken` calls were made (one per `useCommonRoom` join). */
  expectCommonRoomJoinCount(backend: ConnectionServiceBackend, expected: number): void {
    cy.wrap(null).should(() => {
      expect(backend.callsTo(TokenService.method.generateToken).length).to.eq(expected);
    });
  },

  waitForPresenceProbe(): void {
    cy.get("[data-testid='connection-screen-presence-probe']").should("be.visible");
  },
};

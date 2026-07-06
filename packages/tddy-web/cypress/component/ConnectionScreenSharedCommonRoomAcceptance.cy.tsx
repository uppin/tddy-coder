/**
 * Acceptance: ConnectionScreen must reuse SelectedDaemonProvider's common-room LiveKit
 * connection for presence/participants — not open a second join with the same identity.
 *
 * A duplicate join evicts the first participant, disconnecting the room mid-RPC and causing
 * ListSessions (and other daemon-level LiveKit RPC) to fail with "PC manager is closed".
 *
 * PRD context: docs/ft/web/daemon-selector-livekit-rpc.md, docs/ft/web/local-web-dev.md.
 */

import {
  mountConnectionScreenWithProductionCommonRoom,
  sharedCommonRoomPage,
} from "../support/pages/sharedCommonRoomPage";

describe("ConnectionScreen — shared common-room LiveKit connection", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("joins the tddy-lobby common room only once when sharing SelectedDaemonProvider's room", () => {
    // Given — production-like mount: provider + ConnectionScreen-style presence both join the lobby
    const backend = mountConnectionScreenWithProductionCommonRoom({ sessions: [] });

    // When — auth and presence hooks settle
    sharedCommonRoomPage.waitForPresenceProbe();

    // Then — exactly one GenerateToken (one Room.connect path), not two duplicate identities
    sharedCommonRoomPage.expectCommonRoomJoinCount(backend, 1);
  });
});

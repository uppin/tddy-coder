/**
 * Page object for the LiveKit rooms panel on `#/livekit`.
 *
 * All raw selectors live here; test bodies call named methods.
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 */

import {
  byTestId,
  livekitRoomCreatedAt,
  livekitRoomEntry,
  livekitRoomLabel,
  livekitRoomName,
  livekitRoomNoParticipants,
  livekitRoomParticipantCount,
  livekitRoomParticipantEntry,
  livekitRoomParticipantJoined,
  livekitRoomParticipantMetadata,
  livekitRoomParticipantRole,
  livekitRoomParticipantState,
  livekitRoomToggle,
  TEST_IDS,
} from "../testIds";

export const liveKitRoomsPanelPage = {
  // ---------------------------------------------------------------------------
  // Panel root and panel-level states
  // ---------------------------------------------------------------------------

  panel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.livekitRoomsPanel, { timeout: 5000, ...options }),

  loading: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.livekitRoomsPanelLoading, { timeout: 5000, ...options }),

  empty: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.livekitRoomsPanelEmpty, { timeout: 5000, ...options }),

  error: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.livekitRoomsPanelError, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Room rows
  // ---------------------------------------------------------------------------

  room: (room: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomEntry(room), { timeout: 5000, ...options }),

  roomName: (room: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomName(room), { timeout: 5000, ...options }),

  roomLabel: (room: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomLabel(room), { timeout: 5000, ...options }),

  roomParticipantCount: (room: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomParticipantCount(room), { timeout: 5000, ...options }),

  roomCreatedAt: (room: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomCreatedAt(room), { timeout: 5000, ...options }),

  roomNoParticipants: (room: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomNoParticipants(room), { timeout: 5000, ...options }),

  /** Expand (or collapse) a room row to reveal its participants. */
  toggleRoom(room: string) {
    byTestId(livekitRoomToggle(room), { timeout: 5000 }).click();
  },

  // ---------------------------------------------------------------------------
  // Participant rows
  // ---------------------------------------------------------------------------

  participant: (room: string, identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomParticipantEntry(room, identity), { timeout: 5000, ...options }),

  participantRole: (room: string, identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomParticipantRole(room, identity), { timeout: 5000, ...options }),

  participantJoined: (room: string, identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomParticipantJoined(room, identity), { timeout: 5000, ...options }),

  participantState: (room: string, identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomParticipantState(room, identity), { timeout: 5000, ...options }),

  participantMetadata: (room: string, identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(livekitRoomParticipantMetadata(room, identity), { timeout: 5000, ...options }),

  /**
   * Reveal a participant's metadata card the way a keyboard user does.
   *
   * The panel's card opens on pointer-hover and on focus — the Radix tooltip primitive drives both.
   * The component harness cannot synthesize the `pointerenter`/`pointermove` pair Radix listens for
   * (the package ships no `cypress-real-events`), so the suite drives the focus path, which is a
   * real user path rather than a stand-in for one. Mirrors `sessionsDrawerPage`'s tooltip coverage.
   */
  revealMetadata(room: string, identity: string) {
    liveKitRoomsPanelPage.participant(room, identity).focus();
  },

  // ---------------------------------------------------------------------------
  // Assertions
  // ---------------------------------------------------------------------------

  /** Asserts the rooms listed, in the order the panel renders them. */
  expectRoomsInOrder(names: string[]) {
    liveKitRoomsPanelPage
      .panel()
      .find("[data-room-name]")
      .then(($rows) => {
        const rendered = [...$rows].map((row) => row.getAttribute("data-room-name"));
        expect(rendered).to.deep.equal(names);
      });
  },

  /** Asserts the participants listed under one room, in the order the panel renders them. */
  expectParticipantsInOrder(room: string, identities: string[]) {
    liveKitRoomsPanelPage
      .room(room)
      .find("[data-participant-identity]")
      .then(($rows) => {
        const rendered = [...$rows].map((row) => row.getAttribute("data-participant-identity"));
        expect(rendered).to.deep.equal(identities);
      });
  },

  /** Asserts a participant's metadata card shows exactly this text. */
  expectMetadataCard(room: string, identity: string, text: string) {
    liveKitRoomsPanelPage
      .participantMetadata(room, identity)
      .should("be.visible")
      .and("have.text", text);
  },

  /**
   * Asserts the rooms panel renders after the connected-participants panel — the PRD places it
   * below, and DOM order is the only ordering fact a CSS-less harness can observe.
   */
  expectRenderedBelowConnectedParticipants() {
    cy.get(
      `[data-testid='${TEST_IDS.connectedParticipantsPanel}'] ~ [data-testid='${TEST_IDS.livekitRoomsPanel}']`,
    ).should("exist");
  },
};

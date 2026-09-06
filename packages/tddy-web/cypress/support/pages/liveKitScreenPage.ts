/**
 * Page object for the LiveKit screen (`#/livekit`) as a whole — the connected-participants panel,
 * and what the screen says instead of it on a connection that carries no presence.
 *
 * The rooms panel below it has its own page object (`liveKitRoomsPanelPage`).
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const liveKitScreenPage = {
  /** The "Connected participants" panel that wraps the roster. */
  participantsPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectedParticipantsPanel, { timeout: 5000, ...options }),

  /**
   * What a deep link lands on when the host is reached over a wire with no presence: the screen
   * still renders, and names the connection as the reason it has nothing to show.
   */
  unavailable: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.livekitUnavailable, { timeout: 5000, ...options }),
};

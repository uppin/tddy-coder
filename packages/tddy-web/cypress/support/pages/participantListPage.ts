/**
 * Page object for ParticipantList component tests.
 */

import {
  byTestId,
  participantEntry,
  participantRole,
  participantMetadata,
  participantVideoCell,
  participantVideoTrigger,
  participantCodexOauth,
  participantOwnedProjectCount,
  TEST_IDS,
} from "../testIds";

export const participantListPage = {
  list: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.participantList, options),

  empty: () => byTestId(TEST_IDS.participantListEmpty),

  error: () => byTestId(TEST_IDS.participantListError),

  /** What the panel says instead of a roster on a connection that carries no LiveKit presence. */
  unavailable: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.participantListUnavailable, options),

  entry: (identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(participantEntry(identity), options),

  role: (identity: string) => byTestId(participantRole(identity)),

  metadata: (identity: string) => byTestId(participantMetadata(identity)),

  /** The row's camera column cell — absent entirely when the wire carries no tracks. */
  videoCell: (identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(participantVideoCell(identity), options),

  videoTrigger: (identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(participantVideoTrigger(identity), options),

  codexOauth: (identity: string) => byTestId(participantCodexOauth(identity)),

  ownedProjectCount: (identity: string) => byTestId(participantOwnedProjectCount(identity)),

  videoDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId("participant-video-dialog", options),

  videoDialogClose: () => byTestId("participant-video-dialog-close"),

  videoPreview: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId("participant-video-preview", options),

  /**
   * Asserts the panel tells the operator that joining the presence room failed, and quotes the
   * reason LiveKit gave — rather than sitting on the "Connecting…" placeholder.
   */
  expectConnectionFailure(reason: string) {
    participantListPage.error().should("contain.text", reason);
  },
};

/**
 * Page object for ParticipantList component tests.
 */

import {
  byTestId,
  participantEntry,
  participantRole,
  participantMetadata,
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

  entry: (identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(participantEntry(identity), options),

  role: (identity: string) => byTestId(participantRole(identity)),

  metadata: (identity: string) => byTestId(participantMetadata(identity)),

  videoTrigger: (identity: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(participantVideoTrigger(identity), options),

  codexOauth: (identity: string) => byTestId(participantCodexOauth(identity)),

  ownedProjectCount: (identity: string) => byTestId(participantOwnedProjectCount(identity)),

  videoDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId("participant-video-dialog", options),

  videoDialogClose: () => byTestId("participant-video-dialog-close"),

  videoPreview: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId("participant-video-preview", options),
};

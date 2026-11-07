/**
 * Test ids for this package's components, and the one selector helper its page objects use.
 *
 * TODO(livekit-web): `tddy-web/cypress/support/testIds.ts` still carries the same participant-list ids
 * for the specs that stayed there (they drive the roster through the app's screens); the two lists
 * collapse when those screens' specs move — `docs/dev/todo/2026-11-07-participant-list-test-ids-defined-twice.md`.
 */

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> =>
  cy.get(`[data-testid='${id}']`, options);

export const TEST_IDS = {
  participantList: "participant-list",
  participantListEmpty: "participant-list-empty",
  participantListError: "participant-list-error",
  /** Shown instead of the roster when the connection carries no LiveKit presence. */
  participantListUnavailable: "participant-list-unavailable",
} as const;

/** `[data-testid="participant-entry-<identity>"]` */
export const participantEntry = (identity: string) => `participant-entry-${identity}`;

/** `[data-testid="participant-role-<identity>"]` */
export const participantRole = (identity: string) => `participant-role-${identity}`;

/** `[data-testid="participant-metadata-<identity>"]` */
export const participantMetadata = (identity: string) => `participant-metadata-${identity}`;

/** `[data-testid="participant-video-cell-<identity>"]` */
export const participantVideoCell = (identity: string) => `participant-video-cell-${identity}`;

/** `[data-testid="participant-video-trigger-<identity>"]` */
export const participantVideoTrigger = (identity: string) =>
  `participant-video-trigger-${identity}`;

/** `[data-testid="participant-codex-oauth-<identity>"]` */
export const participantCodexOauth = (identity: string) => `participant-codex-oauth-${identity}`;

/** `[data-testid="participant-owned-project-count-<identity>"]` */
export const participantOwnedProjectCount = (identity: string) =>
  `participant-owned-project-count-${identity}`;

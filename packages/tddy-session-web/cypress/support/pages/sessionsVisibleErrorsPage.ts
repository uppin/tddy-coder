/**
 * Page object for the sessions surfaces' failure displays — the drawer's list, the selected
 * session's pane, bulk delete, spawned-child attach, terminal control and terminal input.
 *
 * Navigation (selecting rows, opening the inspector) stays on `sessionsDrawerPage`; this object
 * reads only what an operator is told when something did not work, plus the few actions that
 * provoke those failures.
 *
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  sessionRowSelect,
  sessionsBulkDeleteError,
  sessionsChildAttachError,
  TEST_IDS,
} from "../testIds";

export const sessionsVisibleErrorsPage = {
  /** Why the selected host's session list could not be loaded. */
  listLoadError: () => byTestId(TEST_IDS.sessionsDrawerLoadError, { timeout: 8000 }),

  /** Why the selected session's last attach, resume, delete or terminate failed. */
  paneError: () => byTestId(TEST_IDS.sessionsPaneError, { timeout: 8000 }),

  /** Why one row of a bulk delete was not deleted. */
  bulkDeleteError: (sessionId: string) => byTestId(sessionsBulkDeleteError(sessionId), { timeout: 8000 }),

  /** Why a spawned child conversation could not be attached. */
  childAttachError: (sessionId: string) => byTestId(sessionsChildAttachError(sessionId), { timeout: 8000 }),

  /** Why a manual "Claim terminal" did not take control. */
  claimError: () => byTestId(TEST_IDS.terminalClaimError, { timeout: 8000 }),

  /** The terminal's "input not delivered" indicator. */
  inputError: () => byTestId(TEST_IDS.terminalInputError, { timeout: 8000 }),

  /** The real terminal canvas, once it is ready to take keystrokes. */
  readyTerminal: () =>
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("have.attr", "data-terminal-ready", "true"),

  /** Type into the real terminal (clicking it first focuses ghostty-web's canvas). */
  typeIntoTerminal(text: string) {
    byTestId(TEST_IDS.ghosttyTerminal).click().type(text);
  },

  /** Turn on selection mode, tick `sessionIds`, and delete them. */
  bulkDelete(sessionIds: string[]) {
    byTestId(TEST_IDS.sessionsDrawerSelectMode).click();
    for (const id of sessionIds) byTestId(sessionRowSelect(id)).check();
    byTestId(TEST_IDS.sessionsDrawerBulkDelete).click();
  },
};

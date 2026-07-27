/**
 * Page object for the "Session agents" section acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  sessionAgentsRow,
  sessionAgentsSwitchBtn,
  TEST_IDS,
} from "../testIds";

export const sessionAgentsPage = {
  /** The "Add agent" button in the session-detail header. */
  addAgentBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionAgentsAddBtn, { timeout: 5000, ...options }),

  /** The "Session agents" section root. */
  section: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionAgentsSection, { timeout: 5000, ...options }),

  /** The empty-state message shown when the current session has no peers. */
  emptyState: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionAgentsEmpty, { timeout: 5000, ...options }),

  /** A single peer row for the given peer session id. */
  peerRow: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionAgentsRow(sessionId), { timeout: 5000, ...options }),

  /** The "switch" action on a peer row. */
  peerSwitchBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionAgentsSwitchBtn(sessionId), { timeout: 5000, ...options }),

  /** The peer session ids of every rendered peer row, in DOM order. */
  peerRowSessionIds: (): Cypress.Chainable<string[]> =>
    sessionAgentsPage
      .section()
      .find("[data-testid^='session-agents-row-']")
      .then(($rows) =>
        [...$rows].map((el) => el.getAttribute("data-testid")!.replace("session-agents-row-", "")),
      ),
};

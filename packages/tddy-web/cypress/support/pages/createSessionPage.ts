/**
 * Page object for the CreateSessionPane (new-session form) acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)` in test files.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const createSessionPage = {
  // ---------------------------------------------------------------------------
  // Host selector (multi-daemon)
  // ---------------------------------------------------------------------------

  /** The "Host" `<select>` — which daemon/host runs the session. */
  hostSelect: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionHostSelect, { timeout: 5000, ...options }),

  /** The daemon-instance ids offered as host options, in option order. */
  hostOptionValues: (): Cypress.Chainable<string[]> =>
    createSessionPage
      .hostSelect()
      .find("option")
      .then(($opts) => [...$opts].map((el) => (el as HTMLOptionElement).value)),

  /** Choose which host runs the session. */
  selectHost(daemonInstanceId: string) {
    byTestId(TEST_IDS.createSessionHostSelect).select(daemonInstanceId);
  },

  // ---------------------------------------------------------------------------
  // Core fields
  // ---------------------------------------------------------------------------

  /** Choose the project. */
  selectProject(projectId: string) {
    byTestId(TEST_IDS.createSessionProjectSelect).select(projectId);
  },

  /** Choose the agent (tool sessions). */
  selectAgent(agentId: string) {
    byTestId(TEST_IDS.createSessionAgentSelect).select(agentId);
  },

  /** Switch the branch mode to "work on an existing branch", which triggers branch listing. */
  switchToWorkOnExistingBranch() {
    byTestId(TEST_IDS.createSessionBranchIntentSelect).select("work_on_selected_branch");
  },

  /** Submit the new-session form. */
  submit() {
    byTestId(TEST_IDS.createSessionSubmitBtn).click();
  },
};

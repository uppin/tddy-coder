/**
 * Page object for the failures the new-session form's pickers and the per-session status streams
 * report — each one rendered beside the surface it emptied or froze, never as an empty list.
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)` in test files.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const loadAndStreamErrorsPage = {
  // ---------------------------------------------------------------------------
  // New-session form pickers
  // ---------------------------------------------------------------------------

  /** Why the Project picker has nothing to offer. */
  createSessionProjectsError: () =>
    byTestId(TEST_IDS.createSessionProjectsLoadError, { timeout: 5000 }),

  /** Why a tool session has no tool. */
  createSessionToolsError: () => byTestId(TEST_IDS.createSessionToolsLoadError, { timeout: 5000 }),

  /** Why the stack pickers have no sessions to offer. */
  createSessionSessionsError: () =>
    byTestId(TEST_IDS.createSessionSessionsLoadError, { timeout: 5000 }),

  /** Why the "Branch to work on" picker has nothing to offer. */
  createSessionBranchesError: () =>
    byTestId(TEST_IDS.createSessionBranchesLoadError, { timeout: 5000 }),

  /** The branches failure is not (or no longer) shown. */
  expectNoCreateSessionBranchesError() {
    byTestId(TEST_IDS.createSessionBranchesLoadError).should("not.exist");
  },

  /** The tools failure is not shown. */
  expectNoCreateSessionToolsError() {
    byTestId(TEST_IDS.createSessionToolsLoadError).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // Sessions drawer
  // ---------------------------------------------------------------------------

  /** Why the drawer could not read the project registry. */
  drawerProjectsError: () =>
    byTestId(TEST_IDS.sessionsDrawerProjectsLoadError, { timeout: 5000 }),

  /** Why the drawer's notification dots are no longer live. */
  drawerNotificationsError: () =>
    byTestId(TEST_IDS.sessionsDrawerNotificationsError, { timeout: 5000 }),

  // ---------------------------------------------------------------------------
  // Session streams
  // ---------------------------------------------------------------------------

  /** Why the Usage tab stopped updating. */
  usageStreamError: () => byTestId(TEST_IDS.sessionsUsageStreamError, { timeout: 5000 }),

  /** Why the activity stream stopped. */
  activityStreamError: () => byTestId(TEST_IDS.sessionActivityStreamError, { timeout: 5000 }),

  /** The spec's activity probe shows `count` records — the ones delivered before a failure. */
  expectActivityRecordsShown(count: number) {
    cy.contains(`${count} records`).should("be.visible");
  },

  /** The activity stream failure is not (or no longer) shown. */
  expectNoActivityStreamError() {
    byTestId(TEST_IDS.sessionActivityStreamError).should("not.exist");
  },
};

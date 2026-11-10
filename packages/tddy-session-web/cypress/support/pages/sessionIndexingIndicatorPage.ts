/**
 * Page object for the session header's code-index indicator (`SessionIndexingIndicator`).
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const sessionIndexingIndicatorPage = {
  /** The indicator — present while the session's code index warms, or after its warm failed. */
  indicator: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionIndexingIndicator, options),
};

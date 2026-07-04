/**
 * Page object for the top-right `DaemonSelector` (shadcn `Select`).
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import { byTestId, daemonSelectorOption, TEST_IDS } from "../testIds";

export const daemonSelectorPage = {
  /** The selector's trigger button (shows the currently selected daemon's label). */
  trigger: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.daemonSelectorTrigger, { timeout: 5000, ...options }),

  /** Opens the dropdown. */
  open() {
    daemonSelectorPage.trigger().click();
  },

  /** Visible labels of every option, in list order. Opens the dropdown first. */
  optionLabels(): Cypress.Chainable<string[]> {
    daemonSelectorPage.open();
    return cy
      .get("[role='option']", { timeout: 5000 })
      .should("have.length.greaterThan", 0)
      .then(($opts) => [...$opts].map((el) => el.textContent?.trim() ?? ""));
  },

  /** Selects the daemon with the given instance id. Opens the dropdown first. */
  choose(instanceId: string) {
    daemonSelectorPage.open();
    byTestId(daemonSelectorOption(instanceId)).click();
  },

  /** Asserts the trigger currently shows the given daemon label as the active selection. */
  expectShowsSelected(label: string) {
    daemonSelectorPage.trigger().should("contain.text", label);
  },

  /** Asserts the selector has no daemon selected — the "Select daemon" placeholder is shown. */
  expectEmpty() {
    daemonSelectorPage.trigger().should("contain.text", "Select daemon");
  },
};

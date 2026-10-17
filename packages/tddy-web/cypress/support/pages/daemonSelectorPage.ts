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

  /**
   * Asserts the trigger shows exactly this label and nothing more.
   *
   * Use when the assertion is about how the label was *rendered* rather than which daemon is
   * selected — `expectShowsSelected` is a substring match, so it cannot tell `"udoo"` from
   * `"udoo (this daemon)"` and would pass whether or not the self-label suffix was stripped.
   */
  expectShowsSelectedExactly(label: string) {
    daemonSelectorPage.trigger().should("have.text", label);
  },

  /** Asserts the selector has no daemon selected — the "Select daemon" placeholder is shown. */
  expectEmpty() {
    daemonSelectorPage.trigger().should("contain.text", "Select daemon");
  },

  /**
   * Asserts the selector says the daemon list is empty *because the common room could not be
   * joined*, not because no daemon is running — the distinction the "Select daemon" placeholder
   * hides.
   */
  expectUnreachable() {
    daemonSelectorPage.trigger().should("contain.text", "Common room unreachable");
  },

  /**
   * Asserts the selector is holding on to a selected daemon that is not listed right now: the
   * trigger names it — exactly, so a bare instance id beside the label would fail — and the
   * pulsing dot says "Waiting for daemon".
   */
  expectWaitingFor(label: string) {
    daemonSelectorPage.trigger().should("have.text", label);
    daemonSelectorPage
      .trigger()
      .find(`[data-testid="${TEST_IDS.daemonSelectorWaiting}"]`)
      .should("have.attr", "title", "Waiting for daemon");
  },

  /** Asserts the selected daemon is listed — no waiting dot on the trigger. */
  expectNotWaiting() {
    daemonSelectorPage.trigger().find(`[data-testid="${TEST_IDS.daemonSelectorWaiting}"]`).should("not.exist");
  },

  /**
   * Clicks the option for `instanceId` even if it is disabled — the way a test proves a disabled
   * option really does nothing, rather than only looking disabled. Opens the dropdown first.
   */
  forceChoose(instanceId: string) {
    daemonSelectorPage.open();
    byTestId(daemonSelectorOption(instanceId)).click({ force: true });
  },

  /**
   * Asserts the open dropdown lists `instanceId` as an option that cannot be chosen. Does not open
   * it: a disabled option is reached through {@link forceChoose}, which leaves the dropdown open.
   */
  expectOptionUnavailable(instanceId: string) {
    byTestId(daemonSelectorOption(instanceId)).should("have.attr", "aria-disabled", "true");
  },
};

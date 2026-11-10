/**
 * Page object for the playground's URL state, backed by the shared navigation double
 * (`../navigation`) rather than `window.location`: the package's specs mount the screen with
 * that double as its navigation port.
 *
 * All reads and writes of the location live here; test bodies call named methods.
 */

import { playgroundNavigation } from "../navigation";

export const appLocationPage = {
  /** Put the screen at `location` (e.g. `/rpc-playground?service=s&method=m`) before mounting. */
  startAt(location: string) {
    cy.then(() => {
      playgroundNavigation.navigate(location, { replace: true });
    });
  },

  /** Simulate an inbound location change the screen did not initiate — a pasted deep link. */
  navigateExternally(location: string) {
    cy.then(() => {
      playgroundNavigation.navigate(location);
    });
  },

  /** Assert the location param `key` equals `value`. Retried, as a click lands a tick later. */
  expectParam(key: string, value: string) {
    cy.wrap(null, { log: false }).should(() => {
      expect(playgroundNavigation.location().params[key], `location param "${key}"`).to.equal(value);
    });
  },
};

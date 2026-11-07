/**
 * Page object over a `aNavigationDouble(...)`: the URL-state specs' assertions, read from the
 * navigation port instead of `window.location.hash` (the package never touches the hash itself).
 */
import type { aNavigationDouble } from "tddy-client-web/testkit";

type Navigation = ReturnType<typeof aNavigationDouble>;

export function navigationPage(nav: Navigation) {
  return {
    /** Assert the current location's path equals `path`; retried like a `.should()`. */
    expectPath(path: string) {
      cy.wrap(null, { log: false }).should(() => {
        expect(nav.location().path, "location path").to.equal(path);
      });
    },
    /** Assert the location's query param `key` equals `value`. */
    expectParam(key: string, value: string) {
      cy.wrap(null, { log: false }).should(() => {
        expect(nav.location().params[key], `location param "${key}"`).to.equal(value);
      });
    },
    /**
     * The browser Back button: the double has no history stack to pop, so this navigates to the entry
     * before the current one in its recorded history — the location Back would restore.
     */
    goBack() {
      cy.then(() => {
        const previous = nav.history[nav.history.length - 2];
        expect(previous, "a previous history entry to go back to").to.exist;
        nav.navigate(previous);
      });
    },
  };
}

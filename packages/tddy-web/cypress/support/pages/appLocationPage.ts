/**
 * Page object for the app's URL state (the hash router).
 *
 * All reads and writes of `window.location` live here; test bodies call named methods.
 * No raw `window.location` access in test files — only these named helpers.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 */

/** Split `#/sessions/abc?host=h1` into its path and its hash-local query params. */
function currentLocation(): { path: string; params: URLSearchParams } {
  const hash = window.location.hash.slice(1) || "/";
  const queryAt = hash.indexOf("?");
  return queryAt === -1
    ? { path: hash, params: new URLSearchParams() }
    : { path: hash.slice(0, queryAt), params: new URLSearchParams(hash.slice(queryAt + 1)) };
}

export const appLocationPage = {
  // ---------------------------------------------------------------------------
  // Arrange
  // ---------------------------------------------------------------------------

  /**
   * Put the app at `hash` (written without the leading `#`, e.g. `/sessions/abc?host=h1`) before
   * mounting. Use for deep-link tests.
   */
  startAt(hash: string) {
    window.location.hash = hash;
  },

  /**
   * Reset the hash to the sessions root. Component tests share one window, so a hash left behind by
   * a previous test would otherwise pre-select a session in this one.
   */
  reset() {
    window.location.hash = "/sessions";
  },

  // ---------------------------------------------------------------------------
  // Act
  // ---------------------------------------------------------------------------

  /** Press the browser Back button. */
  goBack() {
    cy.window().then((win) => win.history.back());
  },

  /** Press the browser Forward button. */
  goForward() {
    cy.window().then((win) => win.history.forward());
  },

  /**
   * Simulate an inbound URL change the app did not initiate — an edited address bar, or a link
   * pasted into the already-open tab.
   */
  navigateExternally(hash: string) {
    cy.window().then((win) => {
      win.location.hash = hash;
    });
  },

  // ---------------------------------------------------------------------------
  // Assert
  // ---------------------------------------------------------------------------

  /**
   * Assert the hash path (everything before `?`) equals `path`. Wrapped in `.should()` so Cypress
   * retries — a navigation triggered by a click lands a tick after the click resolves.
   */
  expectPath(path: string) {
    cy.wrap(null, { log: false }).should(() => {
      expect(currentLocation().path, "hash path").to.equal(path);
    });
  },

  /** Assert the hash-local query param `key` equals `value`. */
  expectParam(key: string, value: string) {
    cy.wrap(null, { log: false }).should(() => {
      expect(currentLocation().params.get(key), `hash param "${key}"`).to.equal(value);
    });
  },

  /** Assert the hash-local query param `key` is absent. */
  expectNoParam(key: string) {
    cy.wrap(null, { log: false }).should(() => {
      expect(currentLocation().params.get(key), `hash param "${key}"`).to.equal(null);
    });
  },
};

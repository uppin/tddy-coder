/**
 * Page object for the host-connection probes (`HostConnectionAcceptance.cy.tsx`).
 *
 * The "screen" here is a handful of probe components rather than a product screen — the connection
 * model is a seam, and the spec drives it directly. It still gets a page object: all raw selectors
 * live here, test bodies call named methods, and the ids themselves live in `testIds.ts`.
 *
 * The method names say what the probe *means* rather than what it renders, so a spec body reads as
 * a claim about the connection model ("the host answered with two sessions") instead of as a claim
 * about a div.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const hostConnectionProbePage = {
  // ---------------------------------------------------------------------------
  // Session-count probe — did the resolved client actually reach the host?
  // ---------------------------------------------------------------------------

  /** What the session-count probe currently reports. */
  sessionCount: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.hostConnectionSessionCount, { timeout: 5000, ...options }),

  /** Assert the host answered the list RPC with `count` sessions. */
  shouldHaveListedSessions(count: number) {
    hostConnectionProbePage.sessionCount().should("have.text", `sessions: ${count}`);
  },

  /**
   * Assert the probe was handed no client — the registry could not reach the host, or no host was
   * selected. Reachable only through the probe's effect, never as its initial state, so this
   * cannot pass on first paint before the hooks have answered.
   */
  shouldHaveNoClient() {
    hostConnectionProbePage.sessionCount().should("have.text", "no client");
  },

  // ---------------------------------------------------------------------------
  // Client-identity probe — is a client as stable as the routing that produced it?
  // ---------------------------------------------------------------------------

  /** Re-render the client-identity probe `times` times, without changing its host. */
  reRender(times: number) {
    for (let i = 0; i < times; i += 1) byTestId(TEST_IDS.hostConnectionReRender).click();
  },

  /** Assert how many *distinct* client instances the probe has been handed across its renders. */
  shouldHaveBeenHandedDistinctClients(count: number) {
    byTestId(TEST_IDS.hostConnectionDistinctClients, { timeout: 5000 }).should(
      "have.text",
      String(count),
    );
  },

  /**
   * Assert the probe rendered `count` times.
   *
   * Pinned alongside the distinct-client count so "one client" cannot be satisfied by a probe that
   * never re-rendered at all: a stable identity is only evidence if something asked for it again.
   */
  shouldHaveRendered(count: number) {
    byTestId(TEST_IDS.hostConnectionRenderCount, { timeout: 5000 }).should(
      "have.text",
      String(count),
    );
  },

  // ---------------------------------------------------------------------------
  // Resolved-provider probe — which wire answered?
  // ---------------------------------------------------------------------------

  /** Assert the named provider is the one that issued the connection. */
  shouldHaveResolvedThrough(providerId: string) {
    byTestId(TEST_IDS.hostConnectionResolvedProvider, { timeout: 5000 }).should(
      "have.text",
      providerId,
    );
  },

  /** Assert no registered provider could reach the host. */
  shouldBeUnreachable() {
    byTestId(TEST_IDS.hostConnectionResolvedProvider, { timeout: 5000 }).should(
      "have.text",
      "unreachable",
    );
  },
};

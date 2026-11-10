/**
 * Page object for the Host Stats Footer acceptance tests.
 *
 * The footer is the screen-level bottom strip on `SessionsDrawerScreen` that hosts the
 * relocated byte-traffic readout plus the host-level disk and per-core CPU indicators.
 * All raw selectors live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/host-stats-footer.md
 */

import { byTestId, cpuCoreBar, TEST_IDS } from "../testIds";

export const hostStatsFooterPage = {
  /** The screen-level bottom footer container. */
  footer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.hostStatsFooter, { timeout: 5000, ...options }),

  /** The available-disk-space readout inside the footer. */
  diskAvailable: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.diskSpaceAvailable, { timeout: 5000, ...options }),

  /** The container holding the per-core CPU mini bars. */
  cpuCores: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.cpuCores, { timeout: 5000, ...options }),

  /** All per-core CPU mini bars. */
  cpuCoreBars: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get(`[data-testid^='cpu-core-bar-']`, { timeout: 5000, ...options }),

  /** The mini bar for logical core `index` (0-based). */
  cpuCoreBar: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(cpuCoreBar(index), { timeout: 5000, ...options }),

  /** The relocated byte-traffic strip, scoped to inside the footer. */
  trafficStripInFooter: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.hostStatsFooter, { timeout: 5000, ...options }).find(
      `[data-testid='${TEST_IDS.sessionTrafficStrip}']`,
    ),
};

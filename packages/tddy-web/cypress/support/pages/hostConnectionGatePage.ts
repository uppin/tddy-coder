/**
 * Page object for the layout's host connection gate (`HostConnectionGate`, rendered by `AppShell`
 * on host-scoped screens). All raw selectors live here.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const hostConnectionGatePage = {
  /** The overlay covering a host-scoped screen's body. */
  gate: () => byTestId(TEST_IDS.hostConnectionGate, { timeout: 5000 }),

  /** The gate's headline: which host is not reachable, or that it is still connecting. */
  title: () => byTestId(TEST_IDS.hostConnectionGateTitle, { timeout: 5000 }),

  /** Why the host is not reachable. */
  reason: () => byTestId(TEST_IDS.hostConnectionGateReason, { timeout: 5000 }),

  /** The screen body the gate covers. */
  gatedBody: () => byTestId(TEST_IDS.hostConnectionGatedBody, { timeout: 5000 }),

  /** The header's navigation menu button, which stays usable while the body is blocked. */
  navigationMenuButton: () => byTestId(TEST_IDS.shellMenuButton),
};

/**
 * Page object for the serving-daemon probe in `ServingDaemonSandboxedCodebaseAcceptance`.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const servingHostPage = {
  /** The instance id the serving source named. */
  expectHostId(instanceId: string) {
    byTestId(TEST_IDS.servingHostId).should("have.text", instanceId);
  },

  /**
   * What that host's jail confines, as the descriptor carries it: `"true"`, `"false"`, or
   * `"unadvertised"` when the serving daemon described no jail at all.
   */
  expectJail(confinement: "true" | "false" | "unadvertised") {
    byTestId(TEST_IDS.servingHostJail).should("have.text", confinement);
  },

  /** The verdict the Start-Session form reaches from that host — `"available"`, or the reason. */
  expectPlacementVerdict(verdict: string) {
    byTestId(TEST_IDS.servingHostPlacementVerdict).should("have.text", verdict);
  },

  /** The verdict names why the placement cannot be offered. */
  expectPlacementVerdictContains(fragment: string) {
    byTestId(TEST_IDS.servingHostPlacementVerdict).should("contain.text", fragment);
  },
};

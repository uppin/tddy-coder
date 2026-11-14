/**
 * Acceptance: the orchestrator composer (`#orchestrators` 8/11) — the screen a project header's "+"
 * opens — is a prompt box centred vertically in the detail pane, with only five controls under
 * it: model, effort, provider account, host and attachments, the New-session form's own pieces.
 *
 * Uses the in-memory ConnectRPC backend (`mountWithClients` + `anInMemoryRpcBackend`), so the
 * controls render what the daemon double served.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 * Changeset: docs/dev/changesets/2026-11-14-orchestrator-composer.md
 */

import {
  COMPOSER_PROJECT,
  PANE_HEIGHT,
  aDaemonServingAnOrchestratorAgent,
  mountTheComposer,
} from "../support/rpc/orchestratorComposerBackend";
import { orchestratorComposerPage as composer } from "../support/pages/orchestratorComposerPage";
import { TEST_IDS } from "../support/testIds";

/** How far, as a fraction of the pane's height, the box's centre may sit from the pane's. */
const CENTRED_WITHIN = 0.15;

describe("OrchestratorComposer", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("the prompt box is vertically centred in the pane", () => {
    // When
    mountTheComposer(aDaemonServingAnOrchestratorAgent());

    // Then
    composer.assertFillsPaneOf(COMPOSER_PROJECT, PANE_HEIGHT);
    composer.assertBoxVerticallyCentredIn(COMPOSER_PROJECT, CENTRED_WITHIN);
  });

  it("the composer offers exactly model, effort, provider account, host and attachments", () => {
    // When
    mountTheComposer(aDaemonServingAnOrchestratorAgent());

    // Then
    composer
      .selectTestIds(COMPOSER_PROJECT)
      .should("have.members", [
        TEST_IDS.createSessionModelSelect,
        TEST_IDS.createSessionEffortSelect,
        TEST_IDS.createSessionProviderSelect,
        TEST_IDS.createSessionHostSelect,
      ]);
    composer.attachments(COMPOSER_PROJECT).should("exist");
    composer.checkboxes(COMPOSER_PROJECT).should("have.length", 0);
  });
});

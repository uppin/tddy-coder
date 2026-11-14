/**
 * The orchestrator composer's own behaviour (`#orchestrators` 8/11): an empty box sends nothing;
 * Enter sends the text as an orchestrator's first prompt and hands the started session to the
 * screen; the host picker names the host the orchestrator runs on.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 */

import {
  COMPOSER_PROJECT,
  HOST_B,
  STARTED_ORCHESTRATOR,
  aDaemonServingAnOrchestratorAgent,
  mountTheComposer,
} from "../support/rpc/orchestratorComposerBackend";
import { orchestratorComposerPage as composer } from "../support/pages/orchestratorComposerPage";

const ORCHESTRATOR_ROLE = "orchestrator";

describe("OrchestratorComposer — sending", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("an empty box cannot be sent", () => {
    // Given
    const backend = aDaemonServingAnOrchestratorAgent();

    // When
    mountTheComposer(backend);

    // Then
    composer.pane(COMPOSER_PROJECT).should("be.visible");
    composer.sendButton().should("be.disabled");
    composer.assertNothingStarted(backend);
  });

  it("Enter in the box starts an orchestrator with the text and hands it to the screen", () => {
    // Given
    const backend = aDaemonServingAnOrchestratorAgent();
    const onStarted = cy.stub().as("onStarted");
    mountTheComposer(backend, onStarted);
    composer.write("Plan the next stack");

    // When
    composer.pressEnter();

    // Then
    composer.theStartRequest(backend).then((request) => {
      expect({
        sessionRole: request.sessionRole,
        sessionType: request.sessionType,
        projectId: request.projectId,
        initialPrompt: request.initialPrompt,
      }).to.deep.equal({
        sessionRole: ORCHESTRATOR_ROLE,
        sessionType: "claude-cli",
        projectId: COMPOSER_PROJECT,
        initialPrompt: "Plan the next stack",
      });
    });
    cy.get("@onStarted").should("have.been.calledOnceWith", STARTED_ORCHESTRATOR);
  });

  it("the host picker names the host the orchestrator runs on", () => {
    // Given
    const backend = aDaemonServingAnOrchestratorAgent();
    mountTheComposer(backend);
    composer.write("Plan the next stack");

    // When
    composer.chooseHost(HOST_B);
    composer.send();

    // Then
    composer.theStartRequest(backend).its("daemonInstanceId").should("equal", HOST_B);
  });
});

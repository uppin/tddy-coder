/**
 * Acceptance: the orchestrator composer starts with the defaults saved for its project on its host
 * (`ProjectEntry.orchestrator_defaults`, saved from the project's settings pane — `#orchestrators`
 * 11/11). The operator can still change any control for one start, which saves nothing; a control
 * whose default was cleared starts at what the agent itself serves.
 *
 * Uses the in-memory ConnectRPC backend (`mountWithClients` + `anInMemoryRpcBackend`): the defaults
 * arrive on the daemon double's `ListProjects` row, and the assertions read the `StartSession`
 * request the composer sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 * Changeset: docs/dev/1-WIP/2026-11-10-project-settings-pane.md
 */

import { create } from "@bufbuild/protobuf";
import { OrchestratorDefaultsSchema } from "tddy-client-web/gen/project_pb";
import {
  HOST_A,
  aDaemonServingAnOrchestratorAgent,
  mountTheComposer,
} from "../support/rpc/orchestratorComposerBackend";
import { orchestratorComposerPage as composer } from "../support/pages/orchestratorComposerPage";

/** The agent's own model, as the daemon double serves it. */
const AGENTS_DEFAULT_MODEL = "opus";
/** The provider select's value for the agent's own vendor. */
const ITS_VENDOR = "builtin";

/** A daemon whose row for the composer's project on {@link HOST_A} carries these saved defaults. */
function aDaemonWithSavedDefaults(model: string, effort: string, providerAccountId: string) {
  return aDaemonServingAnOrchestratorAgent({
    orchestratorDefaults: create(OrchestratorDefaultsSchema, { model, effort, providerAccountId }),
  });
}

describe("OrchestratorComposer defaults", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("saved defaults pre-fill the composer and can be changed for one start", () => {
    // Given
    const backend = aDaemonWithSavedDefaults("sonnet", "high", "acct-meta");
    mountTheComposer(backend);
    composer.modelSelect().should("have.value", "sonnet");
    composer.effortSelect().should("have.value", "high");
    composer.providerSelect().should("have.value", "acct-meta");

    // When the operator lowers the effort for this start only
    composer.chooseEffort("low");
    composer.write("Plan the next stack");
    composer.send();

    // Then the start carries the saved defaults and the change, and no default was saved
    composer.theStartRequest(backend).should((request) => {
      expect(request.model, "model").to.equal("sonnet");
      expect(request.effort, "effort").to.equal("low");
      expect(request.providerAccountId, "provider account").to.equal("acct-meta");
      expect(request.daemonInstanceId, "host").to.equal(HOST_A);
    });
    composer.assertNoDefaultsSaved(backend);
  });

  it("a cleared default falls back to the agent's default", () => {
    // Given only an effort is saved: the model and provider account were cleared
    const backend = aDaemonWithSavedDefaults("", "high", "");

    // When
    mountTheComposer(backend);

    // Then the saved effort is pre-filled, and the cleared controls start at what the agent serves
    composer.effortSelect().should("have.value", "high");
    composer.modelSelect().should("have.value", AGENTS_DEFAULT_MODEL);
    composer.providerSelect().should("have.value", ITS_VENDOR);
    composer.write("Plan the next stack");
    composer.send();
    composer.theStartRequest(backend).should((request) => {
      expect(request.model, "model").to.equal(AGENTS_DEFAULT_MODEL);
      expect(request.effort, "effort").to.equal("high");
      expect(request.providerAccountId, "provider account").to.equal("");
    });
  });
});

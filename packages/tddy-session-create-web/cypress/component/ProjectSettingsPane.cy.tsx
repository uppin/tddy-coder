/**
 * `ProjectSettingsPane` on its own (`#orchestrators` 11/11): main's section — the setup form, or a
 * set-up main's Replace main — and the orchestrator defaults section, which starts from the saved
 * defaults and saves them with `SetProjectOrchestratorDefaults` for the pane's host.
 *
 * In-memory ConnectRPC backend (`mountWithClients` + `anInMemoryRpcBackend`): the daemon double
 * serves the orchestrator agent the composer's specs use, and records the save.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { aClientsPortDouble } from "tddy-client-web/testkit";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  OrchestratorDefaultsSchema,
  ProjectEntrySchema,
  ProjectService,
  SetProjectOrchestratorDefaultsResponseSchema,
} from "tddy-client-web/gen/project_pb";
import { ProjectSettingsPane, type ProjectSettingsProject } from "../../src/ProjectSettingsPane";
import {
  COMPOSER_HOSTS,
  COMPOSER_PROJECT,
  HOST_A,
  HOST_B,
  aDaemonServingAnOrchestratorAgent,
} from "../support/rpc/orchestratorComposerBackend";
import { AGENT_DEFAULT, projectSettingsPanePage as settings } from "../support/pages/projectSettingsPanePage";

const PROJECT = COMPOSER_PROJECT;
const MAIN_SESSION = "home-beta-0000-0000-0000-000000000000";

function aProject(fields: Partial<ProjectSettingsProject> = {}): ProjectSettingsProject {
  return create(ProjectEntrySchema, {
    projectId: PROJECT,
    name: "beta",
    mainBranchRef: "origin/main",
    homeSessionId: "",
    ...fields,
  });
}

const savedDefaults = (model: string, effort: string, providerAccountId: string) =>
  create(OrchestratorDefaultsSchema, { model, effort, providerAccountId });

/** The orchestrator agent's daemon, answering a save with the row, or refusing it with `refusal`. */
function aDaemon(refusal?: ConnectError): InMemoryRpcBackend {
  return aDaemonServingAnOrchestratorAgent().onUnary(
    ProjectService.method.setProjectOrchestratorDefaults,
    (request) => {
      if (refusal) throw refusal;
      return create(SetProjectOrchestratorDefaultsResponseSchema, {
        project: create(ProjectEntrySchema, {
          projectId: request.projectId,
          daemonInstanceId: request.daemonInstanceId,
          orchestratorDefaults: request.defaults,
        }),
      });
    },
  );
}

function mountThePane(
  backend: InMemoryRpcBackend,
  {
    project = aProject(),
    host = HOST_A,
    replacing = false,
    onDefaultsSaved = cy.stub(),
  }: {
    project?: ProjectSettingsProject | null;
    host?: string;
    replacing?: boolean;
    onDefaultsSaved?: (project: unknown) => void;
  } = {},
) {
  cy.mountWithClients(
    <ProjectSettingsPane
      projectId={PROJECT}
      project={project}
      replacing={replacing}
      daemonInstanceId={host}
      sessionToken="tok-settings"
      onStartMain={cy.stub().resolves()}
      onDefaultsSaved={onDefaultsSaved}
      onClose={cy.stub()}
    />,
    backend,
    { clients: aClientsPortDouble(backend, { daemons: COMPOSER_HOSTS }) },
  );
}

describe("ProjectSettingsPane", () => {
  beforeEach(() => {
    cy.viewport(1280, 900);
  });

  describe("main", () => {
    it("a project with no main shows main's setup form and no Replace main", () => {
      // When
      mountThePane(aDaemon());

      // Then
      settings.mainForm(PROJECT).should("contain", "Set up main for beta");
      settings.assertNoReplaceMain(PROJECT);
    });

    it("a set-up main offers Replace main, which opens main's form replacing it", () => {
      // Given
      mountThePane(aDaemon(), { project: aProject({ homeSessionId: MAIN_SESSION }) });

      // When
      settings.clickReplaceMain(PROJECT);

      // Then
      settings.replacingNotice(PROJECT).should("be.visible");
      settings.mainForm(PROJECT).should("contain", "Replace main for beta");
    });

    it("opened replacing, main's form says the current main is stopped", () => {
      // When
      mountThePane(aDaemon(), { project: aProject({ homeSessionId: MAIN_SESSION }), replacing: true });

      // Then
      settings.replacingNotice(PROJECT).should("be.visible");
    });

    it("a host that does not list the project says so", () => {
      // When
      mountThePane(aDaemon(), { project: null });

      // Then
      settings.pane(PROJECT).should("exist");
      settings.unknownProject(PROJECT).should("be.visible");
    });
  });

  describe("orchestrator defaults", () => {
    it("starts from the defaults saved for the project", () => {
      // When
      mountThePane(aDaemon(), {
        project: aProject({ orchestratorDefaults: savedDefaults("sonnet", "high", "acct-meta") }),
      });

      // Then
      settings.modelSelect(PROJECT).should("have.value", "sonnet");
      settings.effortSelect(PROJECT).should("have.value", "high");
      settings.providerSelect(PROJECT).should("have.value", "acct-meta");
    });

    it("saving sends the chosen defaults for the project on the pane's host", () => {
      // Given a pane acting on the second host
      const backend = aDaemon();
      const onDefaultsSaved = cy.stub().as("onDefaultsSaved");
      mountThePane(backend, { host: HOST_B, onDefaultsSaved });
      settings.chooseDefaults(PROJECT, { model: "sonnet", effort: "high", provider: "acct-meta" });

      // When
      settings.save(PROJECT);

      // Then
      settings.theSaveRequest(backend).should((request) => {
        expect(request.projectId, "project").to.equal(PROJECT);
        expect(request.daemonInstanceId, "host").to.equal(HOST_B);
        expect(request.defaults?.model, "model").to.equal("sonnet");
        expect(request.defaults?.effort, "effort").to.equal("high");
        expect(request.defaults?.providerAccountId, "provider account").to.equal("acct-meta");
      });
      cy.get("@onDefaultsSaved").should("have.been.calledOnce");
    });

    it("saving with every control at the agent's default clears the defaults", () => {
      // Given defaults saved for every control
      const backend = aDaemon();
      mountThePane(backend, {
        project: aProject({ orchestratorDefaults: savedDefaults("sonnet", "high", "acct-meta") }),
      });
      settings.chooseDefaults(PROJECT, AGENT_DEFAULT);

      // When
      settings.save(PROJECT);

      // Then
      settings.theSaveRequest(backend).should((request) => {
        expect(request.defaults, "no defaults: cleared").to.equal(undefined);
      });
    });

    it("a refused save shows the daemon's reason and keeps the choice", () => {
      // Given
      mountThePane(aDaemon(new ConnectError("project not found", Code.NotFound)));
      settings.chooseDefaults(PROJECT, { model: "sonnet", effort: "low", provider: AGENT_DEFAULT.provider });

      // When
      settings.save(PROJECT);

      // Then
      settings.defaultsError(PROJECT).should("contain", "project not found");
      settings.modelSelect(PROJECT).should("have.value", "sonnet");
      settings.effortSelect(PROJECT).should("have.value", "low");
    });
  });
});

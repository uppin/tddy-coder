/**
 * `ProjectSettingsPane` on its own (`#orchestrators` 11/11): main's section — the setup form, or a
 * set-up main's Replace main — the orchestrator defaults section, which starts from the saved
 * defaults and saves them with `SetProjectOrchestratorDefaults` for the pane's host, and the
 * project itself: its name (`UpdateProject`) and **Delete project** (`DeleteProject`, a soft delete
 * that keeps the project's data, behind a confirmation that says so).
 *
 * In-memory ConnectRPC backend (`mountWithClients` + `anInMemoryRpcBackend`): the daemon double
 * serves the orchestrator agent the composer's specs use, and records the save.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 * Changeset: docs/dev/1-WIP/2026-11-15-project-update-and-soft-delete.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { aClientsPortDouble } from "tddy-client-web/testkit";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DeleteProjectResponseSchema,
  OrchestratorDefaultsSchema,
  ProjectEntrySchema,
  ProjectService,
  SetProjectOrchestratorDefaultsResponseSchema,
  UpdateProjectResponseSchema,
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

/**
 * The orchestrator agent's daemon, answering a save, a rename and a delete with the row — or
 * refusing them with `refusal`.
 */
function aDaemon(refusal?: ConnectError): InMemoryRpcBackend {
  return aDaemonServingAnOrchestratorAgent()
    .onUnary(ProjectService.method.setProjectOrchestratorDefaults, (request) => {
      if (refusal) throw refusal;
      return create(SetProjectOrchestratorDefaultsResponseSchema, {
        project: create(ProjectEntrySchema, {
          projectId: request.projectId,
          daemonInstanceId: request.daemonInstanceId,
          orchestratorDefaults: request.defaults,
        }),
      });
    })
    .onUnary(ProjectService.method.updateProject, (request) => {
      if (refusal) throw refusal;
      return create(UpdateProjectResponseSchema, {
        project: create(ProjectEntrySchema, { projectId: request.projectId, name: request.name.trim() }),
        appliedHosts: [HOST_A, HOST_B],
      });
    })
    .onUnary(ProjectService.method.deleteProject, (request) => {
      if (refusal) throw refusal;
      return create(DeleteProjectResponseSchema, {
        project: create(ProjectEntrySchema, { projectId: request.projectId, deletedAt: "2026-11-15T10:00:00.000Z" }),
        appliedHosts: [HOST_A, HOST_B],
      });
    });
}

function mountThePane(
  backend: InMemoryRpcBackend,
  {
    project = aProject(),
    host = HOST_A,
    replacing = false,
    onDefaultsSaved = cy.stub(),
    onProjectChanged = cy.stub(),
    onDeleted = cy.stub(),
  }: {
    project?: ProjectSettingsProject | null;
    host?: string;
    replacing?: boolean;
    onDefaultsSaved?: (project: unknown) => void;
    onProjectChanged?: (project: unknown) => void;
    onDeleted?: (projectId: string) => void;
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
      onProjectChanged={onProjectChanged}
      onDeleted={onDeleted}
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

  describe("the project", () => {
    it("starts the name field from the project's name", () => {
      mountThePane(aDaemon());

      settings.nameField(PROJECT).should("have.value", "beta");
    });

    it("renaming sends UpdateProject for the project and hands the renamed row back", () => {
      // Given
      const backend = aDaemon();
      const onProjectChanged = cy.stub().as("onProjectChanged");
      mountThePane(backend, { onProjectChanged });

      // When
      settings.rename(PROJECT, "Beta App");

      // Then
      settings.theRenameRequest(backend).should((request) => {
        expect(request.projectId, "project").to.equal(PROJECT);
        expect(request.name, "name").to.equal("Beta App");
        expect(request.localOnly, "fanned out to every host").to.equal(false);
      });
      settings.renamed(PROJECT).should("be.visible");
      cy.get("@onProjectChanged").should("have.been.calledOnce");
    });

    it("a refused rename shows the daemon's reason and keeps the typed name", () => {
      mountThePane(aDaemon(new ConnectError("invalid project name", Code.InvalidArgument)));

      settings.rename(PROJECT, "a/b");

      settings.renameError(PROJECT).should("contain", "invalid project name");
      settings.nameField(PROJECT).should("have.value", "a/b");
    });

    it("Delete asks first, saying plainly that the project's data is kept", () => {
      // Given
      const backend = aDaemon();
      mountThePane(backend);

      // When
      settings.askToDelete(PROJECT);

      // Then the confirmation names the project and says nothing is removed from disk
      settings.deleteConfirmation(PROJECT).should("contain", "beta");
      settings.deleteConfirmation(PROJECT).should("contain", "kept");
      settings.deleteConfirmation(PROJECT).should("contain", "Restore");
      // And nothing was sent yet
      settings.deleteRequests(backend).should("have.length", 0);
    });

    it("cancelling the confirmation keeps the project and sends nothing", () => {
      const backend = aDaemon();
      mountThePane(backend);
      settings.askToDelete(PROJECT);

      settings.cancelDelete(PROJECT);

      settings.assertNoDeleteConfirmation(PROJECT);
      settings.deleteRequests(backend).should("have.length", 0);
    });

    it("a confirmed delete sends DeleteProject for the project and reports it deleted", () => {
      // Given
      const backend = aDaemon();
      const onDeleted = cy.stub().as("onDeleted");
      mountThePane(backend, { onDeleted });
      settings.askToDelete(PROJECT);

      // When
      settings.confirmDelete(PROJECT);

      // Then
      cy.get("@onDeleted").should("have.been.calledOnceWith", PROJECT);
      settings.deleteRequests(backend).should((requests) => {
        expect(requests, "one DeleteProject request").to.have.length(1);
        expect(requests[0]?.projectId, "project").to.equal(PROJECT);
        expect(requests[0]?.localOnly, "fanned out to every host").to.equal(false);
      });
    });

    it("a refused delete shows the daemon's reason, naming the running sessions", () => {
      // Given a daemon refusing because a session still runs
      const onDeleted = cy.stub().as("onDeleted");
      mountThePane(
        aDaemon(new ConnectError("project beta has running sessions: busy-1 on host-b", Code.FailedPrecondition)),
        { onDeleted },
      );
      settings.askToDelete(PROJECT);

      // When
      settings.confirmDelete(PROJECT);

      // Then
      settings.deleteError(PROJECT).should("contain", "busy-1 on host-b");
      cy.get("@onDeleted").should("not.have.been.called");
    });
  });
});

/**
 * Acceptance (`#workqueue` 1/14): the `pr-stack` workflow recipe is removed. The New Session form no
 * longer offers it (or its legacy aliases), and the stack base-session picker that hung off it is gone
 * for every recipe — even when an old link pre-selects the removed recipe.
 *
 * Feature: docs/ft/coder/workflow-recipes.md
 */

import { aClientsPortDouble } from "tddy-client-web/testkit";
import React from "react";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/CreateSessionPane";
import { SessionService } from "tddy-client-web/gen/session_pb";
import { ProjectService } from "tddy-client-web/gen/project_pb";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { createSessionPage } from "../support/pages/createSessionPage";

const PROJECT_ID = "proj-recipes";
const REMOVED_STACK_RECIPES = ["pr-stack", "plan-pr-stack", "orchestrate-pr-stack"];

/** A session on a branch — the kind the removed picker used to offer as a stack base. */
const A_SESSION_ON_A_BRANCH = {
  sessionId: "session-on-branch",
  projectId: PROJECT_ID,
  recipe: "tdd",
  orchestratorSessionId: "",
  branch: "feat/on-branch",
  daemonInstanceId: "local",
};

function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [
        {
          projectId: PROJECT_ID,
          name: "recipes-project",
          gitUrl: "https://example.com/recipes.git",
          mainRepoPath: "/home/dev/recipes-project",
          mainBranchRef: "origin/master",
          daemonInstanceId: "local",
        },
      ],
    }))
    .onUnary(CatalogService.method.listAgents, () =>
      withDeclaredSessionTypes({
        agents: [{ id: "claude", label: "Claude", ...servedAgentWire("claude") }],
      }),
    )
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", version: "0.1.0" }],
    }))
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [A_SESSION_ON_A_BRANCH] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }));
}

function mountPaneWithRecipe(backend: ReturnType<typeof aCreateSessionBackend>, recipe: string) {
  cy.mountWithClients(
    <CreateSessionPane
      client={createClient(SessionService, backend.transport())}
      projectClient={createClient(ProjectService, backend.transport())}
      catalogClient={createClient(CatalogService, backend.transport())}
      sessionFilesClient={createClient(SessionFilesService, backend.transport())}
      worktreeClient={createClient(WorktreeService, backend.transport())}
      sessionToken="fake-token"
      onCancel={cy.stub()}
      onCreated={cy.stub()}
      initialValues={{ sessionType: "tool", projectId: PROJECT_ID, recipe }}
    />,
    backend,
    {
      clients: aClientsPortDouble(backend, {
        daemons: [{ instanceId: "local", label: "local (this daemon)" }],
      }),
    },
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("the recipe dropdown offers no PR stack recipe", () => {
  // Given
  const backend = aCreateSessionBackend();

  // When
  mountPaneWithRecipe(backend, "tdd");

  // Then
  createSessionPage.expectRecipesNotOffered(REMOVED_STACK_RECIPES);
});

it("the start form shows no stack base-session picker for any recipe", () => {
  // Given — an old link that still pre-selects the removed recipe
  const backend = aCreateSessionBackend();

  // When
  mountPaneWithRecipe(backend, "pr-stack");
  createSessionPage.recipeSelect().should("be.visible");

  // Then
  createSessionPage.expectNoPrStackBaseSessionPicker();
});

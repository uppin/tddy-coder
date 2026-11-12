/**
 * Acceptance: the New-session form's **Session orchestrator** switch (`#orchestrators` 1/11). On, it
 * asks `StartSession` for `session_role: "orchestrator"` and hides the placement, branch and stack
 * fields, as main's setup form does — the daemon places an orchestrator on the project's home
 * worktree itself. It is offered only for an agent whose spec declares it can be held read-only
 * (`project_home_main`).
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`), so the test asserts on the typed
 * `StartSession` request the form actually sent.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md
 * Changeset: docs/dev/changesets/2026-11-12-orchestrator-role.md
 */
import React from "react";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/CreateSessionPane";
import { SessionService } from "tddy-client-web/gen/session_pb";
import { ProjectService } from "tddy-client-web/gen/project_pb";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { createSessionPage } from "../support/pages/createSessionPage";
import { TEST_IDS, byTestId } from "../support/testIds";

const PROJECT_ID = "proj-orchestrated";
const ORCHESTRATOR_ROLE = "orchestrator";

/** A daemon serving Claude (which declares `project_home_main`) and Cursor (which does not). */
function aBackendServingClaudeAndCursor() {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: PROJECT_ID, name: "Orchestrated", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () =>
      withDeclaredSessionTypes({
        agents: [
          { id: "claude", label: "Claude", ...servedAgentWire("claude") },
          { id: "cursor", label: "Cursor", ...servedAgentWire("cursor") },
        ],
      }),
    )
    .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "opus", label: "Opus" }],
      defaultModel: "opus",
    }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "orchestrator-1" }));
}

function mountTheForm(backend: InMemoryRpcBackend) {
  cy.mount(
    <CreateSessionPane
      client={createClient(SessionService, backend.transport())}
      projectClient={createClient(ProjectService, backend.transport())}
      catalogClient={createClient(CatalogService, backend.transport())}
      sessionFilesClient={createClient(SessionFilesService, backend.transport())}
      worktreeClient={createClient(WorktreeService, backend.transport())}
      sessionToken="tok-orchestrator"
      onCancel={cy.stub()}
      onCreated={cy.stub()}
    />,
  );
}

/** The one `StartSession` request the form sent. */
function theStartRequest(backend: InMemoryRpcBackend) {
  return cy.wrap(null).then(() => {
    const calls = backend.callsTo(SessionService.method.startSession);
    expect(calls, "one StartSession request").to.have.length(1);
    return calls[0];
  });
}

describe("CreateSessionPane — Session orchestrator switch", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("the orchestrator switch sends session_role orchestrator and hides placement, branch and stack", () => {
    // Given a Claude CLI session, with its placement and branch fields on show
    const backend = aBackendServingClaudeAndCursor();
    mountTheForm(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    createSessionPage.selectProject(PROJECT_ID);
    byTestId(TEST_IDS.createSessionBranchIntentSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionSandboxToggle).should("be.visible");

    // When the switch is turned on and the session is created
    byTestId(TEST_IDS.createSessionOrchestratorToggle).check();

    // Then the fields the daemon decides for an orchestrator are gone
    byTestId(TEST_IDS.createSessionSandboxToggle).should("not.exist");
    byTestId(TEST_IDS.createSessionSandboxedCodebaseToggle).should("not.exist");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).should("not.exist");
    byTestId(TEST_IDS.createSessionBranchIntentSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionStackParentSelect).should("not.exist");

    // And the start asks for the orchestrator role, naming no placement or branch
    createSessionPage.submit();
    theStartRequest(backend).then((request) => {
      expect({
        sessionType: request.sessionType,
        sessionRole: request.sessionRole,
        sandbox: request.sandbox,
        sandboxedCodebase: request.sandboxedCodebase,
        newBranchName: request.newBranchName,
        stackParent: request.stackParent,
      }).to.deep.equal({
        sessionType: "claude-cli",
        sessionRole: ORCHESTRATOR_ROLE,
        sandbox: false,
        sandboxedCodebase: false,
        newBranchName: "",
        stackParent: "",
      });
    });
  });

  it("the switch is offered only for an agent that can be held read-only", () => {
    // Given
    mountTheForm(aBackendServingClaudeAndCursor());

    // When Claude CLI — whose spec declares project_home_main — is chosen
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then the switch is offered
    byTestId(TEST_IDS.createSessionOrchestratorToggle).should("be.visible").and("not.be.checked");

    // When Cursor CLI — whose spec does not — is chosen instead
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();

    // Then it is not
    byTestId(TEST_IDS.createSessionOrchestratorToggle).should("not.exist");
  });
});

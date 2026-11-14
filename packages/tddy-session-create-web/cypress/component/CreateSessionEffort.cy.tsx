/**
 * The New-session form's Effort picker (`#orchestrators` 3/11): beside Model, it lists the effort
 * levels the target daemon served for the selected agent (`AgentInfo.effort_levels`, read from the
 * agent's YAML spec), sends the choice as `StartSessionRequest.effort`, and is not shown for an
 * agent whose spec declares no levels.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-start-effort.md.
 *
 * Uses the in-memory ConnectRPC backend so the assertions read the typed request the component sent.
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
import { TEST_IDS, byTestId } from "../support/testIds";

// Deliberately unlike claude.yaml's levels, so the picker can only show them if it renders what the
// daemon served — the web keeps no list of levels of its own.
const SERVED_CLAUDE_LEVELS = ["low", "high", "daemon-advertised-level"];

/** A daemon serving claude (with effort levels) and cursor (with none), and one project. */
function aBackendServingEffortLevels(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "opus", label: "Claude Opus" }],
      defaultModel: "opus",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () =>
      withDeclaredSessionTypes({
        agents: [
          {
            id: "claude",
            label: "Claude",
            ...servedAgentWire("claude"),
            effortLevels: SERVED_CLAUDE_LEVELS,
          },
          { id: "cursor", label: "Cursor", ...servedAgentWire("cursor"), effortLevels: [] },
        ],
      }),
    )
    .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "effort-sess-1" }));
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  cy.mount(
    <CreateSessionPane
      client={createClient(SessionService, backend.transport())}
      projectClient={createClient(ProjectService, backend.transport())}
      catalogClient={createClient(CatalogService, backend.transport())}
      sessionFilesClient={createClient(SessionFilesService, backend.transport())}
      worktreeClient={createClient(WorktreeService, backend.transport())}
      sessionToken="tok"
      onCancel={cy.stub()}
      onCreated={cy.stub()}
    />,
  );
}

/** The option values the effort picker offers, in order. */
function effortOptionValues(): Cypress.Chainable<string[]> {
  return byTestId(TEST_IDS.createSessionEffortSelect)
    .find("option")
    .then(($options) => Cypress.$.makeArray($options).map((option) => (option as HTMLOptionElement).value));
}

describe("CreateSession effort picker", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("the effort picker lists the selected agent's levels and sends the choice", () => {
    // Given
    const backend = aBackendServingEffortLevels();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    // When
    byTestId(TEST_IDS.createSessionEffortSelect).select("daemon-advertised-level");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the agent's default first, then the served levels in their order; the choice is sent
    effortOptionValues().should("deep.equal", ["", ...SERVED_CLAUDE_LEVELS]);
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].effort).to.eq("daemon-advertised-level");
    });
  });

  it("the effort picker is hidden for an agent without levels", () => {
    // Given — the picker is shown for claude
    mountCreatePane(aBackendServingEffortLevels());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionEffortSelect).should("be.visible");

    // When
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionModelSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionEffortSelect).should("not.exist");
  });
});

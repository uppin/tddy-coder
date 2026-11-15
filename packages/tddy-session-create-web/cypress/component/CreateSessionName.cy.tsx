/**
 * The New-session form's optional **Name** field: the session's display name, sent trimmed as
 * `StartSessionRequest.display_name` so the drawer row and its notifications carry it from the
 * start. Not shown for a tool session — the daemon refuses a name on a `tddy_coder` launch, whose
 * tddy-coder writes the session's record itself.
 *
 * Uses the in-memory ConnectRPC backend so the assertions read the typed request the component sent.
 */
import React from "react";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/CreateSessionPane";
import { create } from "@bufbuild/protobuf";
import {
  ListSessionsResponseSchema,
  SessionService,
  StartSessionResponseSchema,
} from "tddy-client-web/gen/session_pb";
import { ListProjectsResponseSchema, ProjectService } from "tddy-client-web/gen/project_pb";
import {
  CatalogService,
  ListAgentModelsResponseSchema,
  ListSubagentsResponseSchema,
  ListToolsResponseSchema,
} from "tddy-client-web/gen/catalog_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

/** A daemon serving the tool session type and claude, and one project. */
function aBackendServingClaude(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => create(ListSessionsResponseSchema, {}))
    .onUnary(CatalogService.method.listSubagents, () => create(ListSubagentsResponseSchema, {}))
    .onUnary(CatalogService.method.listAgentModels, () =>
      create(ListAgentModelsResponseSchema, {
        models: [{ id: "opus", label: "Claude Opus" }],
        defaultModel: "opus",
      }),
    )
    .onUnary(ProjectService.method.listProjects, () =>
      create(ListProjectsResponseSchema, {
        projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
      }),
    )
    // The shared wire helper is typed loosely (`launch: string`), as in every spec that uses it.
    .onUnary(CatalogService.method.listAgents, () =>
      withDeclaredSessionTypes({
        agents: [{ id: "claude", label: "Claude", ...servedAgentWire("claude") }],
      }),
    )
    .onUnary(CatalogService.method.listTools, () => create(ListToolsResponseSchema, {}))
    .onUnary(SessionService.method.startSession, () =>
      create(StartSessionResponseSchema, { sessionId: "named-sess-1" }),
    );
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

describe("CreateSession Name field", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("the Name field's text is sent, trimmed, as the session's name", () => {
    // Given
    const backend = aBackendServingClaude();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    // When
    byTestId(TEST_IDS.createSessionNameInput).type("  Fix the login flow  ");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].displayName).to.eq("Fix the login flow");
    });
  });

  it("the Name field is not shown for a tool session", () => {
    // Given — the field is shown for claude
    mountCreatePane(aBackendServingClaude());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionNameInput).should("be.visible");

    // When
    byTestId(TEST_IDS.createSessionTypeToolBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionNameInput).should("not.exist");
  });
});

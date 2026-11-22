/**
 * Acceptance (`#workqueue` 5/14): the New Session form offers "With work queue"; checking it starts the
 * session with a queue it owns (`StartSessionRequest.with_work_queue`).
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-session-work-queue.md
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

const PROJECT_ID = "proj-work-queue";

function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [
        {
          projectId: PROJECT_ID,
          name: "work-queue-project",
          gitUrl: "https://example.com/wq.git",
          mainRepoPath: "/home/dev/work-queue-project",
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
    .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(SessionService.method.startSession, () => ({
      sessionId: "sess-with-queue-1",
      livekitRoom: "room-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }));
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("checking With work queue starts the session with a queue", () => {
  // Given
  const backend = aCreateSessionBackend();
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
      initialValues={{ sessionType: "claude-cli", projectId: PROJECT_ID }}
    />,
    backend,
    { clients: aClientsPortDouble(backend, { daemons: [{ instanceId: "local", label: "local (this daemon)" }] }) },
  );

  // When
  cy.get('[data-testid="create-session-with-work-queue"]', { timeout: 5000 }).check();
  createSessionPage.submit();

  // Then
  cy.wrap(backend)
    .invoke("callsTo", SessionService.method.startSession)
    .should("have.length", 1)
    .its("0.withWorkQueue")
    .should("equal", true);
});

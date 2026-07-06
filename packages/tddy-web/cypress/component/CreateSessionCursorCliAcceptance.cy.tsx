/**
 * Acceptance tests: Cursor CLI session type in the new-session form (`CreateSessionPane`).
 *
 * PRD: docs/ft/daemon/cursor-cli-session.md — third session type with model from
 * `ListAgentModels("cursor-cli")` and `StartSession.sessionType = "cursor-cli"`.
 */
import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { createSessionPage } from "../support/pages/createSessionPage";
import { TEST_IDS, byTestId, createSessionSubagentCheckbox } from "../support/testIds";

const CURSOR_CLI_MODELS = [
  { id: "gpt-5.3-codex", label: "GPT-5.3 Codex" },
  { id: "composer-2.5", label: "Composer 2.5" },
];

function aBackendForCursorCliSession() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({
      subagents: [
        { name: "fastcontext", label: "Fast Context", model: "microsoft/FastContext-1.0-4B-RL" },
        { name: "my-explorer", label: "Explorer", model: "qwen2.5-coder:7b" },
      ],
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-cursor", name: "Cursor Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: ["origin/main"] }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "cursor-cli-sess-1" }))
    .onUnary(ConnectionService.method.listAgentModels, (req) => {
      if (req.agent === "cursor-cli") {
        return { models: CURSOR_CLI_MODELS, defaultModel: "gpt-5.3-codex" };
      }
      return { models: [{ id: "opus", label: "Opus" }], defaultModel: "opus" };
    });
}

function mountCreateSessionPane(backend: InMemoryRpcBackend) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <CreateSessionPane
      client={client}
      sessionToken="tok-cursor"
      onCancel={cy.stub()}
      onCreated={cy.stub().as("onCreated")}
    />,
  );
}

describe("CreateSessionPane — cursor-cli session type", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("exposes Cursor CLI as a third session type with model, sandbox, and managed workflow fields", () => {
    // Given
    mountCreateSessionPane(aBackendForCursorCliSession());

    // When
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).should("have.attr", "aria-pressed", "true");
    byTestId(TEST_IDS.createSessionModelSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionInitialPromptInput).should("be.visible");
    byTestId(TEST_IDS.createSessionPermissionModeSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionAgentSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionSandboxToggle).should("be.visible");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).should("be.visible");
  });

  it("populates the cursor-cli model dropdown from ListAgentModels", () => {
    // Given
    mountCreateSessionPane(aBackendForCursorCliSession());

    // When
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "gpt-5.3-codex");
    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      cy.get("option[value='gpt-5.3-codex']").should("exist");
      cy.get("option[value='composer-2.5']").should("exist");
    });
  });

  it("creates a cursor-cli session with sessionType and model in StartSession", () => {
    // Given
    const backend = aBackendForCursorCliSession();
    mountCreateSessionPane(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();
    createSessionPage.selectProject("proj-cursor");
    byTestId(TEST_IDS.createSessionModelSelect).select("composer-2.5");
    byTestId(TEST_IDS.createSessionInitialPromptInput).type("Fix the race in session hooks");
    createSessionPage.submit();

    // Then
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("cursor-cli");
      expect(calls[0].model).to.eq("composer-2.5");
      expect(calls[0].initialPrompt).to.eq("Fix the race in session hooks");
      expect(calls[0].agent).to.eq("");
      expect(calls[0].toolPath).to.eq("");
    });
    cy.get("@onCreated").should("have.been.calledWith", "cursor-cli-sess-1");
  });

  it("submitting cursor-cli with sandbox and managed workflow sends the matching StartSession fields", () => {
    // Given
    const backend = aBackendForCursorCliSession();
    mountCreateSessionPane(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();
    createSessionPage.selectProject("proj-cursor");
    byTestId(TEST_IDS.createSessionSandboxToggle).click();
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();
    byTestId(createSessionSubagentCheckbox("fastcontext")).click();
    createSessionPage.submit();

    // Then
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("cursor-cli");
      expect(calls[0].sandbox).to.eq(true);
      expect(calls[0].managedCodebase).to.eq(true);
      expect(calls[0].specializedAgents).to.deep.eq(["fastcontext"]);
    });
  });
});

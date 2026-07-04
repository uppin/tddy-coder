/**
 * Acceptance: the Claude-CLI new-session form gains an explicit "Managed codebase" checkbox
 * (see docs/ft/coder/managed-codebase-workflow.md) that, when enabled, reveals BOTH a workflow
 * recipe picker and the specialized-subagent multi-select, and sends an explicit
 * `managed_codebase` flag plus the selected `recipe` on StartSession.
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`) so the test asserts on the typed
 * StartSession request the component actually sent — mirrors CreateSessionManagedCodebase.cy.tsx.
 */
import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { TEST_IDS, byTestId, createSessionSubagentCheckbox } from "../support/testIds";

/** A backend seeded with every RPC CreateSessionPane calls on mount, including one stubbed
 * specialized subagent, plus a StartSession stub. */
function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({
      subagents: [
        { name: "fastcontext", label: "FastContext", model: "microsoft/FastContext-1.0-4B-RL" },
      ],
    }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "managed-wf-1" }));
}

function mountCreatePane(backend: ReturnType<typeof aCreateSessionBackend>) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <CreateSessionPane
      client={client}
      sessionToken="tok"
      onCancel={cy.stub()}
      onCreated={cy.stub()}
    />,
  );
}

describe("CreateSession managed-codebase workflow", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("exposes Managed codebase as a checkbox for claude-cli and hides it for tool sessions", () => {
    // Given — the "tool" session type is selected by default
    mountCreatePane(aCreateSessionBackend());

    // Then — no Managed codebase control for tool sessions
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).should("not.exist");

    // When — switching to claude-cli
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then — the control is a real checkbox (an explicit flag, not just an expander)
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle)
      .should("be.visible")
      .and("have.attr", "type", "checkbox");
  });

  it("enabling Managed codebase reveals the recipe picker and the subagent list", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then — neither the recipe picker nor the subagent list is shown while disabled
    byTestId(TEST_IDS.createSessionRecipeSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionManagedCodebaseSection).should("not.exist");

    // When — enabling Managed codebase
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // Then — both the recipe picker and the subagent list appear
    byTestId(TEST_IDS.createSessionRecipeSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionManagedCodebaseSection).should("be.visible");
    byTestId(createSessionSubagentCheckbox("fastcontext")).should("be.visible");
  });

  it("creating a managed claude-cli session sends managedCodebase=true and the selected recipe", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // When — choose the bugfix recipe and one subagent, then submit
    byTestId(TEST_IDS.createSessionRecipeSelect).select("bugfix");
    byTestId(createSessionSubagentCheckbox("fastcontext")).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the typed StartSession request carried the explicit flag, recipe, and subagent
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].managedCodebase).to.eq(true);
      expect(calls[0].recipe).to.eq("bugfix");
      expect(calls[0].specializedAgents).to.deep.equal(["fastcontext"]);
    });
  });

  it("a managed session with a recipe and no subagents still sends managedCodebase=true and the recipe", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // When — choose a recipe but select no subagents, then submit
    byTestId(TEST_IDS.createSessionRecipeSelect).select("tdd");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — managedCodebase is the explicit flag; no subagents are sent
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].managedCodebase).to.eq(true);
      expect(calls[0].recipe).to.eq("tdd");
      expect(calls[0].specializedAgents).to.deep.equal([]);
    });
  });
});

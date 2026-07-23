/**
 * Acceptance: the managed-codebase section gains a "Semantic index" checkbox
 * (see docs/ft/coder/semantic-index.md). When on, the session's StartSession request carries
 * `semantic_index = true`; the daemon then indexes the worktree before launch and exposes the
 * SemanticSearch tool. The option lives inside the Managed codebase section for both the claude-cli
 * and cursor-cli session types, defaults to off, and never leaks a value from a hidden control.
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`) so the test asserts on the typed
 * StartSession request the component actually sent — mirrors CreateSessionManagedWorkflow.cy.tsx.
 */
import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

/** A backend seeded with every RPC CreateSessionPane calls on mount, plus a StartSession stub. */
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
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "semantic-1" }));
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

describe("CreateSession managed-codebase semantic index", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("hides the Semantic index option until Managed codebase is enabled", () => {
    // Given — a claude-cli session with Managed codebase still disabled
    mountCreatePane(aCreateSessionBackend());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then — the Semantic index checkbox is not shown
    byTestId(TEST_IDS.createSessionSemanticIndexToggle).should("not.exist");

    // When — enabling Managed codebase
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // Then — the Semantic index checkbox appears, unchecked
    byTestId(TEST_IDS.createSessionSemanticIndexToggle)
      .should("be.visible")
      .and("not.be.checked");
  });

  it("shows the Semantic index option under Managed codebase for cursor-cli sessions", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());
    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();

    // When — enabling Managed codebase for a cursor-cli session
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // Then — the Semantic index checkbox is available here too
    byTestId(TEST_IDS.createSessionSemanticIndexToggle)
      .should("be.visible")
      .and("not.be.checked");
  });

  it("defaults Semantic index to off — a managed session sends semanticIndex=false", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // When — submitting without touching the Semantic index checkbox
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — semanticIndex defaults to false
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].managedCodebase).to.eq(true);
      expect(calls[0].semanticIndex).to.eq(false);
    });
  });

  it("creating a managed session with Semantic index enabled sends semanticIndex=true", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();

    // When — enabling Semantic index and submitting
    byTestId(TEST_IDS.createSessionSemanticIndexToggle).check();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the typed StartSession request carried semanticIndex=true
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].managedCodebase).to.eq(true);
      expect(calls[0].semanticIndex).to.eq(true);
    });
  });

  it("disabling Managed codebase clears Semantic index — the request sends semanticIndex=false", () => {
    // Given — a managed session with Semantic index turned on
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).check();
    byTestId(TEST_IDS.createSessionSemanticIndexToggle).check();

    // When — turning Managed codebase back off, then submitting
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).uncheck();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the hidden Semantic index value did not leak into the request
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].managedCodebase).to.eq(false);
      expect(calls[0].semanticIndex).to.eq(false);
    });
  });
});

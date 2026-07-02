/**
 * Acceptance: the Claude CLI new-session form gains a collapsible "Managed codebase" section
 * (see docs/ft/coder/specialized-subagents.md) that lets the user attach one or more specialized
 * subagents to the session. Absent for the "tool" session type.
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`) so the test asserts on the typed
 * `StartSession` request the component actually sent — mirrors
 * `CreateSessionSandboxToggle.cy.tsx`, the newest precedent in this file's sibling suite.
 */
import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { TEST_IDS, byTestId, createSessionSubagentCheckbox } from "../support/testIds";

/** A backend seeded with every RPC `CreateSessionPane` calls on mount, including two stubbed
 * specialized subagents, plus a StartSession stub. */
function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({
      subagents: [
        { name: "fastcontext", label: "FastContext", model: "microsoft/FastContext-1.0-4B-RL" },
        { name: "my-explorer", label: "My Explorer", model: "qwen2.5-coder:7b" },
      ],
    }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "managed-new-1" }));
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

describe("CreateSession managed-codebase specialized-subagent picker", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("does not show the Managed codebase section for tool sessions", () => {
    // Given — the "tool" session type is selected by default
    mountCreatePane(aCreateSessionBackend());

    // Then
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).should("not.exist");
  });

  it("shows a collapsed Managed codebase section for claude-cli sessions", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then — the toggle is present, but the subagent list is not yet expanded
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).should("be.visible");
    byTestId(TEST_IDS.createSessionManagedCodebaseSection).should("not.exist");
  });

  it("expanding Managed codebase lists every subagent returned by ListSubagents", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // When
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).click();

    // Then
    byTestId(TEST_IDS.createSessionManagedCodebaseSection).should("be.visible");
    byTestId(createSessionSubagentCheckbox("fastcontext")).should("be.visible");
    byTestId(createSessionSubagentCheckbox("my-explorer")).should("be.visible");
  });

  it("creating a session with two selected subagents sends managedCodebase and both names", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionManagedCodebaseToggle).click();

    // When
    byTestId(createSessionSubagentCheckbox("fastcontext")).click();
    byTestId(createSessionSubagentCheckbox("my-explorer")).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the typed StartSession request carried managedCodebase + both subagent names
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].managedCodebase).to.eq(true);
      expect(calls[0].specializedAgents).to.deep.equal(["fastcontext", "my-explorer"]);
    });
  });
});

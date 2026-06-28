/**
 * Red: the Claude CLI new-session form exposes a "sandbox" toggle that sets
 * `StartSessionRequest.sandbox = true`.
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`) so the test asserts on the typed
 * `StartSession` request the component actually sent.
 */
import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

/** A backend seeded with every RPC `CreateSessionPane` calls on mount + a StartSession stub. */
function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "sandbox-new-1" }));
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

describe("CreateSession sandbox toggle", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("shows a sandbox toggle in the Claude CLI new-session form", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionSandboxToggle).should("be.visible");
  });

  it("submitting with the sandbox toggle on sends StartSession with sandbox=true", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionSandboxToggle).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the typed StartSession request carried the sandbox flag.
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].sandbox).to.eq(true);
    });
  });
});

/**
 * The Claude CLI new-session form exposes a "Dangerously skip permissions" checkbox that sets
 * `StartSessionRequest.dangerously_skip_permissions = true`. Because the flag is mutually exclusive
 * with `--permission-mode`, checking it disables the permission-mode select.
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
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "skip-perms-new-1" }));
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

describe("CreateSession dangerously-skip-permissions toggle", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("shows the toggle in the Claude CLI new-session form", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionDangerouslySkipPermissionsToggle).should("be.visible");
  });

  it("disables the permission-mode select while the toggle is on (they are mutually exclusive)", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionPermissionModeSelect).should("not.be.disabled");
    byTestId(TEST_IDS.createSessionDangerouslySkipPermissionsToggle).click();

    // Then
    byTestId(TEST_IDS.createSessionPermissionModeSelect).should("be.disabled");
  });

  it("submitting with the toggle on sends StartSession with dangerouslySkipPermissions=true", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionDangerouslySkipPermissionsToggle).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the typed StartSession request carried the flag.
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].dangerouslySkipPermissions).to.eq(true);
    });
  });

  it("leaves dangerouslySkipPermissions=false when the toggle is untouched", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].dangerouslySkipPermissions).to.eq(false);
    });
  });
});

/**
 * Acceptance tests: the shared Start-Session dialog offers a pre-checked "Create Remote Branch"
 * toggle in new-branch mode. Leaving it checked sends `createRemoteBranch = true` on StartSession
 * (the daemon pushes the new branch to origin at session start); unchecking sends `false`.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-07-25-branch-query-and-remote-branch.md.
 * Changeset: docs/dev/1-WIP/2026-07-25-branch-query-and-remote-branch.md.
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-remote-branch";

/**
 * A backend that stubs the model catalog (so Create is enabled) and captures StartSession. All
 * other pane-load RPCs fall through to the testkit's empty defaults.
 */
function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [
        {
          projectId: PROJECT_ID,
          name: "remote-branch-project",
          gitUrl: "https://example.com/remote-branch.git",
          mainRepoPath: "/home/dev/remote-branch-project",
          mainBranchRef: "origin/master",
          daemonInstanceId: "local",
        },
      ],
    }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: "child-remote-branch-1",
      livekitRoom: "room-remote-branch-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }));
}

function mountPane(backend: ReturnType<typeof aCreateSessionBackend>) {
  // The pane takes a Connect client directly (not via a hook), so build one over the in-memory
  // backend's transport — its `callsTo` still records every StartSession the pane issues.
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    withSelectedDaemon(
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
        initialValues={{ sessionType: "claude-cli", projectId: PROJECT_ID }}
      />,
    ),
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("shows the Create Remote Branch toggle pre-checked in new-branch mode", () => {
  // Given
  const backend = aCreateSessionBackend();

  // When
  mountPane(backend);

  // Then
  byTestId(TEST_IDS.createSessionCreateRemoteBranchToggle).should("be.checked");
});

it("sends createRemoteBranch = true when the toggle is left checked", () => {
  // Given
  const backend = aCreateSessionBackend();

  // When
  mountPane(backend);
  byTestId(TEST_IDS.createSessionNewBranchNameInput).clear().type("feature/x/n1");
  byTestId(TEST_IDS.createSessionSubmitBtn).click();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].createRemoteBranch).to.equal(true);
  });
});

it("sends createRemoteBranch = false when the toggle is unchecked", () => {
  // Given
  const backend = aCreateSessionBackend();

  // When
  mountPane(backend);
  byTestId(TEST_IDS.createSessionNewBranchNameInput).clear().type("feature/x/n1");
  byTestId(TEST_IDS.createSessionCreateRemoteBranchToggle).uncheck();
  byTestId(TEST_IDS.createSessionSubmitBtn).click();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].createRemoteBranch).to.equal(false);
  });
});

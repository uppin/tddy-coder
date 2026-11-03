/**
 * The Claude CLI new-session form offers "Run the agent in tmux": checked, the typed `StartSession`
 * request carries `run_in_tmux = true`. The option is exclusive with the sandbox, so it is disabled
 * while the sandbox is on.
 * (Feature: docs/ft/daemon/claude-cli-session.md § tmux mode; `#tmux-agent` 1/3)
 *
 * Uses the in-memory ConnectRPC backend so the assertions read the request the component sent.
 */
import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [] }))
    .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "run-in-tmux-new-1" }));
}

function mountCreatePane(backend: ReturnType<typeof aCreateSessionBackend>) {
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

describe("CreateSession run-in-tmux option", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it('the Create pane offers "Run the agent in tmux" for a Claude CLI session', () => {
    // Given
    mountCreatePane(aCreateSessionBackend());

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionTmuxToggle).should("be.visible");
    cy.contains("Run the agent in tmux").should("be.visible");
  });

  it("checking it sends run_in_tmux on StartSession", () => {
    // Given
    const backend = aCreateSessionBackend();
    mountCreatePane(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionTmuxToggle).check();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].runInTmux).to.eq(true);
    });
  });

  it("the option is disabled while the session is sandboxed", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // When
    byTestId(TEST_IDS.createSessionSandboxToggle).check();

    // Then
    byTestId(TEST_IDS.createSessionTmuxToggle).should("be.disabled");
  });

  const everyCliHarnessType = [
    { sessionType: "cursor-cli", button: TEST_IDS.createSessionTypeCursorCliBtn },
    { sessionType: "opencode-cli", button: TEST_IDS.createSessionTypeOpenCodeCliBtn },
    { sessionType: "muse-cli", button: TEST_IDS.createSessionTypeMuseCliBtn },
    { sessionType: "warp-cli", button: TEST_IDS.createSessionTypeWarpCliBtn },
  ];

  it('the Create pane offers "Run the agent in tmux" for every CLI harness type', () => {
    // Given
    mountCreatePane(aCreateSessionBackend());

    everyCliHarnessType.forEach(({ button }) => {
      // When
      byTestId(button).click();

      // Then
      byTestId(TEST_IDS.createSessionTmuxToggle).should("be.visible");
      cy.contains("Run the agent in tmux").should("be.visible");
    });
  });

  everyCliHarnessType
    .filter(({ sessionType }) => ["opencode-cli", "muse-cli", "warp-cli"].includes(sessionType))
    .forEach(({ sessionType, button }) => {
      it(`${sessionType} sends run_in_tmux when checked`, () => {
        // Given
        const backend = aCreateSessionBackend();
        mountCreatePane(backend);

        // When
        byTestId(button).click();
        byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
        byTestId(TEST_IDS.createSessionTmuxToggle).check();
        byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

        // Then
        cy.wrap(null).should(() => {
          const calls = backend.callsTo(SessionService.method.startSession);
          expect(calls).to.have.length(1);
          expect(calls[0].sessionType).to.eq(sessionType);
          expect(calls[0].runInTmux).to.eq(true);
        });
      });
    });
});

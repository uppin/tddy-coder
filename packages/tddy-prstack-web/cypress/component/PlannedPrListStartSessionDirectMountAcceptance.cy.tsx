/**
 * Acceptance: starting a planned PR from the planned-PR list opens the create-session dialog
 * (`tddy-session-create-web`) pre-filled from the node, and starts nothing until the operator submits.
 *
 * The list reports the click and `PrStackScreen` owns the dialog, so the spec mounts the screen on
 * its own: one in-memory backend answers the dialog's own loads, with no drawer in the tree. The
 * drawer-driven twin is `PrStackStartSessionModalAcceptance`.
 *
 * PRD: docs/ft/web/session-drawer.md § PR-Stack Chat Screen.
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { ProjectService } from "tddy-client-web/gen/project_pb";
import { SessionEntrySchema, SessionService } from "tddy-client-web/gen/session_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { PrStackScreen } from "../../src/PrStackScreen";
import { mountWithPresenterRoom } from "../support/mountWorkflowChat";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

const NODE_TITLE = "Add token store";
const NODE_DESCRIPTION = "Persist and retrieve auth tokens";
const NODE_BRANCH = "feature/auth-stack/token-store";

const ORCHESTRATOR = create(SessionEntrySchema, {
  sessionId: "pr-stack-start-direct-0000-0000-0000-000000000001",
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  projectId: "proj-pr-stack",
  recipe: "pr-stack",
  stackPlanJson: aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: NODE_TITLE, description: NODE_DESCRIPTION, branchSuggestion: NODE_BRANCH }),
  ]),
});

/** The backend answers what the dialog loads on mount; a StartSession is recorded, never expected here. */
function aDaemon() {
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-pr-stack", name: "pr-stack-project", mainRepoPath: "/home/dev/pr-stack-project", daemonInstanceId: "local" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({ models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }], defaultModel: "claude-opus-4-8" }))
    .onUnary(CatalogService.method.listTools, () => ({ tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "child-session", livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }));
}

function mountScreenOver(backend: ReturnType<typeof aDaemon>) {
  const transport = backend.transport();
  mountWithPresenterRoom(
    <PrStackScreen
      session={ORCHESTRATOR}
      client={createClient(SessionService, transport)}
      projectClient={createClient(ProjectService, transport)}
      catalogClient={createClient(CatalogService, transport)}
      prStackClient={createClient(PrStackService, transport)}
      sessionFilesClient={createClient(SessionFilesService, transport)}
      worktreeClient={createClient(WorktreeService, transport)}
      sessionToken="fake-token"
    />,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("opens the create-session dialog when Start session is clicked, without starting a session yet", () => {
  // Given
  const backend = aDaemon();
  mountScreenOver(backend);

  // When
  prStackScreenPage.startSessionBtn("n1").click();

  // Then
  prStackScreenPage.createSessionDialog().should("be.visible");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(SessionService.method.startSession)).to.have.length(0);
  });
});

it("pre-fills the planned PR's branch and prompt in the dialog", () => {
  // Given
  mountScreenOver(aDaemon());

  // When
  prStackScreenPage.startSessionBtn("n1").click();

  // Then
  prStackScreenPage.dialogNewBranchNameInput().should("have.value", NODE_BRANCH);
  prStackScreenPage.dialogInitialPromptInput().should("have.value", `${NODE_TITLE}\n\n${NODE_DESCRIPTION}`);
});

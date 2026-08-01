/**
 * Acceptance tests: only one mutation of a planned PR's branch runs at a time.
 *
 * Repoint and "pull the base in" are offered from different parts of the same row — repoint from the
 * always-visible header, merge/rebase from the detail body — and the state where both appear is the
 * *normal* post-merge one: a node becomes repointable exactly when the parent whose merge also left
 * it behind its new base landed.
 *
 * They are not independent. A repoint rebases and force-pushes the node's branch; a pull merges or
 * rebases the base into that same branch. The daemon serializes neither, so the two running side by
 * side leaves a half-rebased worktree or force-pushes over a merge commit. Git's `index.lock` usually
 * makes one abort rather than corrupt — an abort mid-rebase is not a safe outcome either.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Panel UX (C5, D30–D33).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import {
  aPlannedNode,
  aStackPlanJson,
  aBranchResolutionResponse,
  type BranchResolutionFixture,
  type StackNodeFixture,
} from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7600-0000-0000-0000-000000000076";
const PROJECT_ID = "proj-pr-stack";
const DEFAULT_BRANCH = "origin/master";

const ROOT_BRANCH = "feature/auth/token-store";
const MERGED_BRANCH = "feature/auth/session-cookie";
const CHILD_BRANCH = "feature/auth/middleware";

/**
 * A chain whose middle node has merged, which is what makes the last node both repointable *and*
 * behind the base it is still stacked on — the state in which both controls are offered at once.
 */
function aStackWithAMergedMiddleNode(): StackNodeFixture[] {
  return [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: ROOT_BRANCH }),
    aPlannedNode({
      nodeId: "n2",
      title: "Rotate session cookies",
      branch: MERGED_BRANCH,
      parents: ["n1"],
      prStatus: { phase: "merged" },
    }),
    aPlannedNode({
      nodeId: "n3",
      title: "Add auth middleware",
      branch: CHILD_BRANCH,
      parents: ["n2"],
    }),
  ];
}

function anOrchestratorSession(stackPlanJson: string): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson,
  };
}

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: DEFAULT_BRANCH,
  daemonInstanceId: "local",
};

/** The child branch, cleanly behind the base it is stacked on — so a pull is offered. */
const CHILD_BEHIND_ITS_BASE: BranchResolutionFixture = {
  branch: CHILD_BRANCH,
  worktree: { exists: true, path: "/home/dev/worktrees/middleware" },
  baseSync: { baseBranch: ROOT_BRANCH, behindCount: 3, aheadCount: 1 },
};

/** The root branch, behind the default branch — a second node with a pull of its own to offer. */
const ROOT_BEHIND_THE_DEFAULT_BRANCH: BranchResolutionFixture = {
  branch: ROOT_BRANCH,
  worktree: { exists: true, path: "/home/dev/worktrees/token-store" },
  baseSync: { baseBranch: DEFAULT_BRANCH, behindCount: 2, aheadCount: 5 },
};

const RESOLUTION_BY_BRANCH: Record<string, BranchResolutionFixture> = {
  [CHILD_BRANCH]: CHILD_BEHIND_ITS_BASE,
  [ROOT_BRANCH]: ROOT_BEHIND_THE_DEFAULT_BRANCH,
};

/** A call that is accepted and then never answers — the operation stays in flight for the whole test. */
const neverAnswers = () => new Promise<never>(() => undefined);

function aPrStackBackend() {
  return aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, aStackWithAMergedMiddleNode())),
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(RESOLUTION_BY_BRANCH[req.branch] ?? { branch: req.branch }),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));
}

function openPrStackScreen(backend: ReturnType<typeof aPrStackBackend>) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
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
// One branch, one mutation
// ---------------------------------------------------------------------------

it("disables the repoint control while a pull into the same branch is in flight", () => {
  // Given — the pull is merging the base into exactly the branch a repoint would rebase
  openPrStackScreen(
    aPrStackBackend().onUnary(ConnectionService.method.pullBaseIntoBranch, neverAnswers),
  );
  prStackScreenPage.expandRow("n3");

  // When
  prStackScreenPage.clickSyncMerge("n3");

  // Then
  prStackScreenPage.repointBtn("n3").should("be.disabled");
});

it("disables both pull controls while a repoint of the same branch is in flight", () => {
  // Given — the repoint is rebasing and force-pushing exactly the branch a pull would merge into
  openPrStackScreen(
    aPrStackBackend().onUnary(ConnectionService.method.repointPlannedPr, neverAnswers),
  );
  prStackScreenPage.expandRow("n3");

  // When
  prStackScreenPage.clickRepoint("n3");

  // Then
  prStackScreenPage.syncMergeBtn("n3").should("be.disabled");
  prStackScreenPage.syncRebaseBtn("n3").should("be.disabled");
});

it("leaves another node's pull controls enabled while one node's branch is being mutated", () => {
  // Given — mutations of different nodes touch different branches and may legitimately overlap
  openPrStackScreen(
    aPrStackBackend().onUnary(ConnectionService.method.repointPlannedPr, neverAnswers),
  );
  prStackScreenPage.expandRow("n3");
  prStackScreenPage.expandRow("n1");

  // When
  prStackScreenPage.clickRepoint("n3");

  // Then
  prStackScreenPage.syncMergeBtn("n1").should("be.enabled");
});

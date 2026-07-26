/**
 * Acceptance tests: a planned PR whose base branch is not available to be based upon shows a blocked
 * "Missing branch" indicator instead of a Start-session CTA.
 *
 * A child worktree is created from `origin/<base>`, so a base branch absent from the remote makes the
 * spawn fail inside `git fetch` — after `StartSession` was already accepted and a session directory
 * written. The row reads the base branch's remote state from `QueryBranch`'s `remote` leg and blocks
 * up front, naming the branch it is waiting for.
 *
 * An unanswered poll must never block: `useQueryBranch` swallows failures, so treating "unknown" as
 * "missing" would create a permanent dead end of exactly the kind this feature removes.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md (C2, D4–D6).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type SessionEntry } from "../../src/gen/connection_pb";
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7777-0000-0000-0000-000000000070";
const PROJECT_ID = "proj-pr-stack";
/** The predecessor's branch — the base `n2` would be branched from. */
const BASE_BRANCH = "feature/attach-docs/attach-proto";

/** `n1` owns a created branch; `n2` depends on it and has not been started. */
const A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT: StackNodeFixture[] = [
  aPlannedNode({
    nodeId: "n1",
    title: "Start-session attachment proto",
    branch: BASE_BRANCH,
    sessionId: "child-n1",
  }),
  aPlannedNode({
    nodeId: "n2",
    title: "Copy attachments during StartSession",
    branchSuggestion: "feature/attach-docs/attach-start",
    parents: ["n1"],
  }),
];

function anOrchestratorSession(nodes: StackNodeFixture[]): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-26T09:50:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, nodes),
  };
}

function openPrStackScreen(
  nodes: StackNodeFixture[],
  resolutionByBranch: Record<string, BranchResolutionFixture>,
) {
  const backend = aSessionsDrawerBackend([anOrchestratorSession(nodes)]).onUnary(
    ConnectionService.method.queryBranch,
    (req: { branch: string }) =>
      aBranchResolutionResponse(resolutionByBranch[req.branch] ?? { branch: req.branch }),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
}

/** Open the screen with a `QueryBranch` call that never answers. */
function openPrStackScreenWithUnansweredResolution(nodes: StackNodeFixture[]) {
  const backend = aSessionsDrawerBackend([anOrchestratorSession(nodes)]).onUnary(
    ConnectionService.method.queryBranch,
    () => new Promise<never>(() => undefined),
  );
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
// Blocked on a base branch that is absent from the remote
// ---------------------------------------------------------------------------

it("blocks Start session and names the base branch when that branch is absent from the remote", () => {
  // Given — n2's base is n1's branch, which does not exist on origin
  // When
  openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: { branch: BASE_BRANCH, remote: { exists: false } },
  });

  // Then
  prStackScreenPage.missingBranch("n2").should("contain.text", BASE_BRANCH);
  prStackScreenPage.startSessionBtn("n2").should("not.exist");
});

it("offers Start session once the base branch exists on the remote", () => {
  // Given — the same stack, with n1's branch pushed to origin
  // When
  openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: {
      branch: BASE_BRANCH,
      remote: { exists: true, sha: "4e2e8e8cf5de99f8485e518e925d382ae9275c76" },
    },
  });

  // Then
  prStackScreenPage.startSessionBtn("n2").should("exist");
  prStackScreenPage.missingBranch("n2").should("not.exist");
});

it("blocks Start session when no ancestor owns a created branch yet", () => {
  // Given — n1 holds only a planned branch name, so there is no ref for n2 to be based onto
  const nodes = [
    aPlannedNode({
      nodeId: "n1",
      title: "Start-session attachment proto",
      branchSuggestion: BASE_BRANCH,
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Copy attachments during StartSession",
      parents: ["n1"],
    }),
  ];

  // When
  openPrStackScreen(nodes, {});

  // Then — a suggestion names no ref; the daemon refuses such a spawn, so the row must not offer it
  prStackScreenPage.missingBranch("n2").should("exist");
  prStackScreenPage.startSessionBtn("n2").should("not.exist");
});

it("blocks Start session when one of several parents owns no branch, naming that parent's branch", () => {
  // Given — n3 depends on both n1 (branch pushed) and n2 (planned only)
  const nodes = [
    aPlannedNode({
      nodeId: "n1",
      title: "Start-session attachment proto",
      branch: BASE_BRANCH,
      sessionId: "child-n1",
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Session attachment storage",
      branchSuggestion: "feature/attach-docs/attach-store",
    }),
    aPlannedNode({
      nodeId: "n3",
      title: "Copy attachments during StartSession",
      parents: ["n1", "n2"],
    }),
  ];

  // When — n1's branch is on the remote, so only n2 is unmet
  openPrStackScreen(nodes, {
    [BASE_BRANCH]: {
      branch: BASE_BRANCH,
      remote: { exists: true, sha: "4e2e8e8cf5de99f8485e518e925d382ae9275c76" },
    },
  });

  // Then — the daemon refuses on *any* branchless non-merged parent, so a good sibling branch must
  // not mask the unmet one; the row names the branch it is waiting for
  prStackScreenPage
    .missingBranch("n3")
    .should("contain.text", "feature/attach-docs/attach-store");
  prStackScreenPage.startSessionBtn("n3").should("not.exist");
});

// ---------------------------------------------------------------------------
// Never block what cannot be checked
// ---------------------------------------------------------------------------

it("offers Start session for a root node, whose base is the project default branch", () => {
  // Given — a single root node with no parents and no branch of its own
  const nodes = [aPlannedNode({ nodeId: "n1", title: "Start-session attachment proto" })];

  // When
  openPrStackScreen(nodes, {});

  // Then — the default branch exists by construction, so a root is always startable
  prStackScreenPage.startSessionBtn("n1").should("exist");
  prStackScreenPage.missingBranch("n1").should("not.exist");
});

it("offers Start session while the base branch resolution has not arrived", () => {
  // Given / When — QueryBranch never answers, so the base branch's remote state is unknown
  openPrStackScreenWithUnansweredResolution(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT);

  // Then — an unanswered poll must not wedge a node whose predecessor already owns a branch
  prStackScreenPage.startSessionBtn("n2").should("exist");
  prStackScreenPage.missingBranch("n2").should("not.exist");
});

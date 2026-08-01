/**
 * Acceptance tests: a planned-PR row never states a base comparison that is out of date.
 *
 * The row's badge and its merge/rebase controls are rendered from one cached `QueryBranch` resolution
 * per branch, refreshed on a five-second poll. Two things can make that cache describe a repository —
 * or a question — that no longer exists, and both let the operator click a control that does the
 * wrong thing rather than merely showing a stale number:
 *
 * - **A poll response that lands after a fresher write.** The poll issued before a pull finished was
 *   answered from the refs as they stood *before* it, so applying it on arrival re-offers a merge on
 *   an already-synced branch — and clicking it force-pushes a rebase for nothing.
 * - **A comparison against a base the node no longer has.** The cache is keyed by branch alone, so a
 *   repoint that moves a node onto a different base leaves the old count in place; merging then takes
 *   the branch the node was just moved off, back into it.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md (C5, D28–D33).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7700-0000-0000-0000-000000000077";
const PROJECT_ID = "proj-pr-stack";
const DEFAULT_BRANCH = "origin/master";
const POLL_INTERVAL_MS = 5000;

const ROOT_BRANCH = "feature/auth/token-store";
const MERGED_BRANCH = "feature/auth/session-cookie";
const CHILD_BRANCH = "feature/auth/middleware";

/**
 * A chain whose middle node has merged. That makes the last node both behind the base it is still
 * stacked on (`ROOT_BRANCH`, the nearest non-merged ancestor's branch) and repointable onto the
 * project's default branch — the two situations these tests need from one plan.
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

/** The same plan after a repoint of n3: it is stacked on nothing, so its base is the default branch. */
function aStackWithN3Repointed(): StackNodeFixture[] {
  return aStackWithAMergedMiddleNode().map((node) =>
    node.nodeId === "n3" ? { ...node, parents: [] } : node,
  );
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

/** The child branch, cleanly three commits behind the base it is stacked on. */
const CHILD_BEHIND_ITS_BASE: BranchResolutionFixture = {
  branch: CHILD_BRANCH,
  worktree: { exists: true, path: "/home/dev/worktrees/middleware" },
  baseSync: { baseBranch: ROOT_BRANCH, behindCount: 3, aheadCount: 1 },
};

/** The child branch after the pull: it now contains every commit on its base. */
const CHILD_IN_SYNC_AFTER_THE_PULL: BranchResolutionFixture = {
  branch: CHILD_BRANCH,
  worktree: { exists: true, path: "/home/dev/worktrees/middleware" },
  baseSync: { baseBranch: ROOT_BRANCH, behindCount: 0, aheadCount: 4 },
};

/** The root branch with a live PR — a second, independent fact to watch a poll round arrive by. */
const ROOT_WITH_AN_OPEN_PR: BranchResolutionFixture = {
  branch: ROOT_BRANCH,
  pr: { exists: true, number: 41, url: "https://example.com/pr/41", state: "open" },
};

type BranchResolutionResponse = ReturnType<typeof aBranchResolutionResponse>;

/** A `queryBranch` stub. Takes the base as well as the branch: the pair *is* the question asked. */
type QueryBranchHandler = (req: {
  branch: string;
  baseBranch: string;
}) => BranchResolutionResponse | Promise<BranchResolutionResponse>;

function aPrStackBackend(queryBranch: QueryBranchHandler) {
  return aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, aStackWithAMergedMiddleNode())),
  ])
    .onUnary(ConnectionService.method.queryBranch, queryBranch)
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
// A poll response older than the row's current state
// ---------------------------------------------------------------------------

/**
 * A `queryBranch` that answers each branch's first call at once and withholds every later one until
 * the test releases it — the slow poll a fresher write has to outlive.
 */
function aQueryBranchWhoseSecondRoundIsWithheld(rounds: {
  first: Record<string, BranchResolutionFixture>;
  withheld: Record<string, BranchResolutionFixture>;
}) {
  const answeredOnce = new Set<string>();
  const withheld: (() => void)[] = [];
  const answer = (from: Record<string, BranchResolutionFixture>, branch: string) =>
    aBranchResolutionResponse(from[branch] ?? { branch });

  return {
    handler: ((req) => {
      if (!answeredOnce.has(req.branch)) {
        answeredOnce.add(req.branch);
        return answer(rounds.first, req.branch);
      }
      return new Promise<BranchResolutionResponse>((resolve) => {
        withheld.push(() => resolve(answer(rounds.withheld, req.branch)));
      });
    }) satisfies QueryBranchHandler,
    /** Answer every withheld call, in the order the polls were issued. */
    releaseWithheldRound: () => {
      for (const send of withheld.splice(0)) send();
    },
  };
}

it("keeps a completed pull's result when the poll it overtook finally answers", () => {
  // Given — the second poll round is issued while the branch is still behind, then withheld
  cy.clock();
  const polls = aQueryBranchWhoseSecondRoundIsWithheld({
    first: { [CHILD_BRANCH]: CHILD_BEHIND_ITS_BASE },
    // What that round was answered from: the refs as they stood before the pull ran, plus one
    // unrelated fact on another branch to watch the round land by.
    withheld: { [CHILD_BRANCH]: CHILD_BEHIND_ITS_BASE, [ROOT_BRANCH]: ROOT_WITH_AN_OPEN_PR },
  });
  openPrStackScreen(
    aPrStackBackend(polls.handler).onUnary(ConnectionService.method.pullBaseIntoBranch, () =>
      aBranchResolutionResponse(CHILD_IN_SYNC_AFTER_THE_PULL),
    ),
  );
  prStackScreenPage.expandRow("n3");
  cy.tick(POLL_INTERVAL_MS);
  prStackScreenPage.clickSyncMerge("n3");
  prStackScreenPage.baseInSync("n3").should("exist");

  // When — the poll that was issued before the pull finished answers at last
  cy.then(() => polls.releaseWithheldRound());

  // Then — the round did arrive (the PR link it carried is rendered), and the row it could not
  // speak for is untouched: a second click here would rebase and force-push for nothing
  prStackScreenPage.prLink("n1").should("contain.text", "41");
  prStackScreenPage.baseInSync("n3").should("exist");
  prStackScreenPage.syncMergeBtn("n3").should("not.exist");
});

// ---------------------------------------------------------------------------
// A comparison against a base the node no longer has
// ---------------------------------------------------------------------------

/**
 * A `queryBranch` that answers only the comparisons `answers` names, keyed by the pair the caller
 * asked about. Any other pairing is left unanswered — a comparison the daemon has not made yet.
 */
function aQueryBranchAnsweringOnly(
  answers: Record<string, BranchResolutionFixture>,
): QueryBranchHandler {
  return (req) => {
    const fixture = answers[`${req.branch} ${req.baseBranch}`];
    if (fixture) return aBranchResolutionResponse(fixture);
    return new Promise<BranchResolutionResponse>(() => undefined);
  };
}

it("stops offering a pull against the base a repoint just moved the node off", () => {
  // Given — n3 is three commits behind ROOT_BRANCH, the branch it is stacked on
  openPrStackScreen(
    aPrStackBackend(
      aQueryBranchAnsweringOnly({ [`${CHILD_BRANCH} ${ROOT_BRANCH}`]: CHILD_BEHIND_ITS_BASE }),
    ).onUnary(ConnectionService.method.repointPlannedPr, () => ({
      stackPlanJson: aStackPlanJson(2, aStackWithN3Repointed()),
    })),
  );
  prStackScreenPage.expandRow("n3");
  prStackScreenPage.syncMergeBtn("n3").should("contain.text", ROOT_BRANCH);

  // When — the repoint lands n3 on the default branch, and the new comparison has not answered yet
  prStackScreenPage.clickRepoint("n3");

  // Then — merging now would take the branch n3 was just moved off, back into it
  prStackScreenPage.syncMergeBtn("n3").should("not.exist");
  prStackScreenPage.baseBehind("n3").should("not.exist");
});

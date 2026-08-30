/**
 * Acceptance tests: a planned PR shows its GitHub PR even when no session is available for it.
 *
 * `buildBranchQueries` polled only the branches a node **owns**, so a node whose `branch` was never
 * recorded — because its child was spawned on another host, or because the work is long finished and
 * the session is gone — was never queried at all. Its row stayed silent about a PR that is open,
 * reviewed and mergeable.
 *
 * The `pr` leg is the one leg of `QueryBranch` that survives the host boundary: it asks the GitHub
 * API by head branch rather than reading this daemon's disk. So a branchless node is additionally
 * polled on its `branch_suggestion`, and **only that leg is read**. A suggestion is a planned name,
 * not a ref (D1) — letting it feed base resolution or the spawn gate would unblock a spawn onto
 * something nothing created, which is the exact failure D1 exists to prevent.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D41).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-9200-0000-0000-0000-000000000092";
const PROJECT_ID = "proj-pr-stack";
/** The name the planner gave the node. Nothing on this host has created a ref by it. */
const PLANNED_BRANCH = "feature/attach-docs/attach-proto";

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: "origin/master",
  daemonInstanceId: "local",
};

/** A planned PR that owns no branch and records no session — the row this feature is about. */
const A_BRANCHLESS_NODE: StackNodeFixture = aPlannedNode({
  nodeId: "n1",
  title: "Start-session attachment proto",
  branchSuggestion: PLANNED_BRANCH,
});

/** GitHub answers for the planned name; this host's own git sees nothing. */
const A_PR_ON_THE_PLANNED_BRANCH: BranchResolutionFixture = {
  branch: PLANNED_BRANCH,
  session: { exists: false },
  worktree: { exists: false },
  remote: { exists: false },
  pr: {
    exists: true,
    number: 412,
    url: "https://github.com/acme/pr-stack/pull/412",
    state: "open",
  },
};

/** GitHub could not be asked at all — distinct from "this name has no PR" (D27's rule for the PR leg). */
const A_PR_LOOKUP_THAT_FAILED: BranchResolutionFixture = {
  branch: PLANNED_BRANCH,
  session: { exists: false },
  worktree: { exists: false },
  remote: { exists: false },
  pr: {
    exists: false,
    unavailable: true,
    unavailableReason: "no GitHub credential configured for this project",
  },
};

function anOrchestratorSession(nodes: StackNodeFixture[]): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-30T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, nodes),
  };
}

/**
 * Open the PR-Stack screen, and hand back the branches `QueryBranch` is asked about, in call order.
 *
 * The recorder is created per mount and returned rather than kept at module scope: at module scope
 * its emptiness depends on a `beforeEach` running between two mounts, so a scenario that mounted
 * twice — or a hook that stopped running — would read another scenario's calls as its own.
 */
function openPrStackScreen(
  nodes: StackNodeFixture[],
  resolutionByBranch: Record<string, BranchResolutionFixture>,
): string[] {
  const queriedBranches: string[] = [];
  const backend = aSessionsDrawerBackend([anOrchestratorSession(nodes)])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) => {
      queriedBranches.push(req.branch);
      return aBranchResolutionResponse(resolutionByBranch[req.branch] ?? { branch: req.branch });
    })
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));

  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return queriedBranches;
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
// The PR is acknowledged
// ---------------------------------------------------------------------------

it("polls the planned branch name of a node that owns no branch", () => {
  // Given
  // When
  const queriedBranches = openPrStackScreen([A_BRANCHLESS_NODE], {
    [PLANNED_BRANCH]: A_PR_ON_THE_PLANNED_BRANCH,
  });

  // Then — a node with no ref of its own is still the only branch name the poll asks about. The
  // row's PR state is the synchronization point: it is rendered from the answer, so it cannot be
  // there before the call was made.
  prStackScreenPage.prState("n1").should("have.text", "open");
  cy.wrap(null).should(() => {
    expect([...new Set(queriedBranches)]).to.deep.equal([PLANNED_BRANCH]);
  });
});

it("shows the PR number and state of a planned PR that has no session", () => {
  // Given
  // When
  openPrStackScreen([A_BRANCHLESS_NODE], { [PLANNED_BRANCH]: A_PR_ON_THE_PLANNED_BRANCH });

  // Then
  prStackScreenPage.prLink("n1").should("have.text", "#412");
  prStackScreenPage
    .prLink("n1")
    .should("have.attr", "href", "https://github.com/acme/pr-stack/pull/412");
  prStackScreenPage.prState("n1").should("have.text", "open");
});

it("still offers Start session for a planned PR whose PR already exists", () => {
  // Given — a PR is not a session: the node has never been started from here
  // When
  openPrStackScreen([A_BRANCHLESS_NODE], { [PLANNED_BRANCH]: A_PR_ON_THE_PLANNED_BRANCH });

  // Then
  prStackScreenPage.startSessionBtn("n1").should("exist");
});

// ---------------------------------------------------------------------------
// A suggestion is still not a ref
// ---------------------------------------------------------------------------

it("keeps naming the branch as planned rather than owned when only a PR was found for it", () => {
  // Given
  // When
  openPrStackScreen([A_BRANCHLESS_NODE], { [PLANNED_BRANCH]: A_PR_ON_THE_PLANNED_BRANCH });
  prStackScreenPage.expandRow("n1");

  // Then — `branch` means "a branch that exists" (D1); a PR lookup does not create one
  prStackScreenPage.plannedBranchName("n1").should("contain.text", PLANNED_BRANCH);
  prStackScreenPage.branchName("n1").should("not.exist");
});

it("does not let a PR on a planned name unblock a dependent node", () => {
  // Given — n2 waits on n1's branch; n1 has a PR on its planned name and no branch
  const dependent = aPlannedNode({
    nodeId: "n2",
    title: "Copy attachments during StartSession",
    branchSuggestion: "feature/attach-docs/attach-start",
    parents: ["n1"],
  });

  // When
  openPrStackScreen([A_BRANCHLESS_NODE, dependent], {
    [PLANNED_BRANCH]: A_PR_ON_THE_PLANNED_BRANCH,
  });

  // Then — the daemon's spawn gate reads the node's `branch`, so the row must say the same thing
  prStackScreenPage
    .startWarning("n2")
    .should("have.text", "Start-session attachment proto has not created its branch yet");
});

it("reports that a planned PR's lookup failed rather than that it has no PR", () => {
  // Given — the daemon holds no GitHub credential, so it cannot answer for the planned name
  // When
  openPrStackScreen([A_BRANCHLESS_NODE], { [PLANNED_BRANCH]: A_PR_LOOKUP_THAT_FAILED });

  // Then — silence would read as "no PR opened yet", which is the state this row is here to deny
  prStackScreenPage.prUnavailable("n1").should("have.text", "PR status unavailable");
  prStackScreenPage
    .prUnavailable("n1")
    .should("have.attr", "title", "no GitHub credential configured for this project");
  prStackScreenPage.prLink("n1").should("not.exist");
});

it("shows a PR found for an owned branch only on the node that owns it", () => {
  // Given — n1 created the branch; n2 is branchless and still plans that very name, so both rows
  // would read the same resolution if the row took whichever answer arrived under that name
  const owner = aPlannedNode({
    nodeId: "n1",
    title: "Start-session attachment proto",
    branch: PLANNED_BRANCH,
    sessionId: "child-n1",
  });
  const stillPlanningTheSameName = aPlannedNode({
    nodeId: "n2",
    title: "Copy attachments during StartSession",
    branchSuggestion: PLANNED_BRANCH,
  });

  // When
  openPrStackScreen([owner, stillPlanningTheSameName], {
    [PLANNED_BRANCH]: A_PR_ON_THE_PLANNED_BRANCH,
  });

  // Then — the poll asked about an *owned* branch, so only its owner may state the answer: a row
  // with no branch and no session must never render another node's live PR as its own
  prStackScreenPage.prLink("n1").should("have.text", "#412");
  prStackScreenPage.prLink("n2").should("not.exist");
  prStackScreenPage.prState("n2").should("not.exist");
});

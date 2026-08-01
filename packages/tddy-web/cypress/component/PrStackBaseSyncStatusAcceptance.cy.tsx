/**
 * Acceptance tests: each planned-PR row states how its branch stands against its base.
 *
 * `has-conflicts` exists as an internal-status kind but is never derived — only an agent calling
 * `pr_resolve_conflicts` or `pr_set_status` ever sets it, so in the normal case (the orchestrator
 * idle, the operator looking at the panel) the badge is absent or stale. The comparison is now
 * resolved server-side on the same branch poll, so it is live whether or not the agent is running.
 *
 * Two conflations are load-bearing and are pinned here:
 *
 * - **A comparison that could not be made is not "clean".** It arrives with no commits behind and no
 *   conflicts — byte-identical to a healthy branch — so it carries its own discriminator. This is
 *   the rule that already governs PR status (D12), and conflating the two is exactly how a live open
 *   PR stayed invisible for a day.
 * - **"In sync" is a badge, not silence.** If only the bad states rendered, a healthy row and a row
 *   whose poll has not answered would look identical — the same conflation one level down.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md (C4, D26–D29; AC 12–15).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7400-0000-0000-0000-000000000074";
const PROJECT_ID = "proj-pr-stack";
const DEFAULT_BRANCH = "origin/master";

const ROOT_BRANCH = "feature/auth/token-store";
const CHILD_BRANCH = "feature/auth/middleware";

/** A root off the project default, and a node stacked on it. Both own branches. */
function aTwoNodeStack(): StackNodeFixture[] {
  return [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: ROOT_BRANCH }),
    aPlannedNode({
      nodeId: "n2",
      title: "Add auth middleware",
      branch: CHILD_BRANCH,
      parents: ["n1"],
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

interface MountOptions {
  nodes?: StackNodeFixture[];
  resolutionByBranch?: Record<string, BranchResolutionFixture>;
}

function aPrStackBackend(opts: MountOptions) {
  return aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, opts.nodes ?? aTwoNodeStack())),
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(
        opts.resolutionByBranch?.[req.branch] ?? { branch: req.branch },
      ),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));
}

function openPrStackScreen(opts: MountOptions = {}) {
  const backend = aPrStackBackend(opts);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return backend;
}

/** Open the screen with a `QueryBranch` that never answers — no comparison ever arrives. */
function openPrStackScreenWithUnansweredResolution() {
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, aTwoNodeStack())),
  ])
    .onUnary(ConnectionService.method.queryBranch, () => new Promise<never>(() => undefined))
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));

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
// The four states
// ---------------------------------------------------------------------------

it("shows how many commits a planned PR is behind its base branch", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: { baseBranch: ROOT_BRANCH, behindCount: 3, aheadCount: 2 },
      },
    },
  });

  // Then
  prStackScreenPage.baseBehind("n2").should("contain.text", "3").and("contain.text", ROOT_BRANCH);
});

it("shows a conflict badge when the branch cannot be merged with its base", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: {
          baseBranch: ROOT_BRANCH,
          behindCount: 4,
          hasConflicts: true,
          conflictedPaths: ["src/auth/token.rs", "src/auth/mod.rs"],
        },
      },
    },
  });

  // Then
  prStackScreenPage.baseConflicts("n2").should("exist");
  prStackScreenPage.baseBehind("n2").should("not.exist");
});

it("lists the conflicting paths in the row's detail", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: {
          baseBranch: ROOT_BRANCH,
          behindCount: 4,
          hasConflicts: true,
          conflictedPaths: ["src/auth/token.rs", "src/auth/mod.rs"],
        },
      },
    },
  });

  // When
  prStackScreenPage.expandRow("n2");

  // Then — the badge says there is a problem; the detail says where
  prStackScreenPage
    .baseConflictPaths("n2")
    .should("contain.text", "src/auth/token.rs")
    .and("contain.text", "src/auth/mod.rs");
});

it("shows an in-sync badge when the branch contains every commit on its base", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: { baseBranch: ROOT_BRANCH, behindCount: 0, aheadCount: 5 },
      },
    },
  });

  // Then
  prStackScreenPage.baseInSync("n2").should("contain.text", ROOT_BRANCH);
});

it("reports a conflict even when the branch is not behind its base", () => {
  // Given — a conflict with nothing to merge in is unusual but it is still a conflict, and the
  // behind count must not be what decides whether the operator is told
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: {
          baseBranch: ROOT_BRANCH,
          behindCount: 0,
          hasConflicts: true,
          conflictedPaths: ["src/auth/mod.rs"],
        },
      },
    },
  });

  // Then
  prStackScreenPage.baseConflicts("n2").should("exist");
  prStackScreenPage.baseInSync("n2").should("not.exist");
});

// ---------------------------------------------------------------------------
// Unavailable is not clean
// ---------------------------------------------------------------------------

it("reports the base comparison as unavailable rather than in sync when it could not be made", () => {
  // Given — a failed comparison arrives byte-identical to a healthy one
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: {
          baseBranch: ROOT_BRANCH,
          behindCount: 0,
          aheadCount: 0,
          hasConflicts: false,
          unavailable: true,
          unavailableReason: `base branch '${ROOT_BRANCH}' resolves to no commit`,
        },
      },
    },
  });

  // Then
  prStackScreenPage.baseSyncUnavailable("n2").should("exist");
  prStackScreenPage.baseInSync("n2").should("not.exist");
  prStackScreenPage.baseBehind("n2").should("not.exist");
});

it("carries the daemon's reason on the unavailable badge", () => {
  // Given
  const reason = `base branch '${ROOT_BRANCH}' resolves to no commit`;
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: { baseBranch: ROOT_BRANCH, unavailable: true, unavailableReason: reason },
      },
    },
  });

  // Then
  prStackScreenPage.baseSyncUnavailable("n2").should("have.attr", "title", reason);
});

// ---------------------------------------------------------------------------
// Unknown is not clean either
// ---------------------------------------------------------------------------

it("shows no base status while the branch resolution has not arrived", () => {
  // Given / When
  openPrStackScreenWithUnansweredResolution();

  // Then — the row renders, and says nothing it does not know
  prStackScreenPage.plannedPrRow("n2").should("exist");
  prStackScreenPage.baseInSync("n2").should("not.exist");
  prStackScreenPage.baseBehind("n2").should("not.exist");
  prStackScreenPage.baseConflicts("n2").should("not.exist");
  prStackScreenPage.baseSyncUnavailable("n2").should("not.exist");
});

it("shows no base status for a resolution that carries no comparison at all", () => {
  // Given — a daemon that predates base sync answers QueryBranch with its other four legs only
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        worktree: { exists: true, path: "/home/dev/worktrees/middleware" },
      },
    },
  });

  // Then — an absent leg is unknown, and the rest of the row is unaffected
  prStackScreenPage.baseInSync("n2").should("not.exist");
  prStackScreenPage.baseSyncUnavailable("n2").should("not.exist");
});

// ---------------------------------------------------------------------------
// The row names what was actually compared
// ---------------------------------------------------------------------------

it("names the base the daemon compared against rather than the one the row plans", () => {
  // Given — the node's plan base is n1's branch, but the daemon resolved and compared something
  // else. The counts are meaningless next to a ref they did not come from.
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        baseSync: { baseBranch: "origin/release", behindCount: 2 },
      },
    },
  });

  // Then
  prStackScreenPage.baseBehind("n2").should("contain.text", "origin/release");
});

// ---------------------------------------------------------------------------
// What the poll asks for
// ---------------------------------------------------------------------------

it("asks QueryBranch to compare a stacked node's branch against its predecessor's branch", () => {
  // Given
  const backend = openPrStackScreen();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.queryBranch);
    const forChild = calls.find((c: { branch: string }) => c.branch === CHILD_BRANCH);
    expect(forChild, `a QueryBranch call for ${CHILD_BRANCH}`).to.exist;
    expect(forChild.baseBranch).to.equal(ROOT_BRANCH);
  });
});

it("asks QueryBranch to compare a root node's branch against the project's default branch", () => {
  // Given — a root's base is the project default, which the poll set never used to include at all,
  // so the one comparison every stack has was the one that could never be made
  const backend = openPrStackScreen();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.queryBranch);
    const forRoot = calls.find((c: { branch: string }) => c.branch === ROOT_BRANCH);
    expect(forRoot, `a QueryBranch call for ${ROOT_BRANCH}`).to.exist;
    expect(forRoot.baseBranch).to.equal(DEFAULT_BRANCH);
  });
});

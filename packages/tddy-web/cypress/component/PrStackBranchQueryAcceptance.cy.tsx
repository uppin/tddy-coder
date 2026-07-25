/**
 * Acceptance tests: the PR-Stack "Planned PRs" rows resolve a node by its branch through the new
 * `QueryBranch` RPC, which returns the in-progress session, the on-disk worktree, and the live
 * GitHub PR status in one call. The row renders a worktree indicator, an in-progress badge, and the
 * PR link/state from that resolution, refreshed on the poll interval.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-07-25-branch-query-and-remote-branch.md.
 * Changeset: docs/dev/1-WIP/2026-07-25-branch-query-and-remote-branch.md.
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
} from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-4444-0000-0000-0000-000000000040";
const PROJECT_ID = "proj-pr-stack";

/** The interval (ms) at which the PR-Stack view re-polls branch resolution. */
const POLL_INTERVAL_MS = 5000;

function anOrchestratorSession(stackPlanJson: string): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-05T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson,
  };
}

interface MountOptions {
  sessions: Partial<SessionEntry>[];
  /** Static `QueryBranch` resolution per branch. */
  resolutionByBranch?: Record<string, BranchResolutionFixture>;
  /** Dynamic `QueryBranch` resolution per branch, re-evaluated on every poll. */
  resolutionFactory?: (branch: string) => BranchResolutionFixture;
}

function openPrStackScreen(opts: MountOptions) {
  const backend = aSessionsDrawerBackend(opts.sessions).onUnary(
    ConnectionService.method.queryBranch,
    (req: { branch: string }) => {
      const fx = opts.resolutionFactory
        ? opts.resolutionFactory(req.branch)
        : (opts.resolutionByBranch?.[req.branch] ?? { branch: req.branch });
      return aBranchResolutionResponse(fx);
    },
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
// Worktree resolution
// ---------------------------------------------------------------------------

it("shows the on-disk worktree path for a node whose branch has a worktree", () => {
  // Given — n1 owns feature/x/n1, resolved to a worktree on disk
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    resolutionByBranch: {
      "feature/x/n1": {
        branch: "feature/x/n1",
        worktree: { exists: true, path: "/home/dev/pr-stack-project/.worktrees/feature-x-n1" },
      },
    },
  });

  // Then
  prStackScreenPage
    .worktree("n1")
    .should("contain.text", "/home/dev/pr-stack-project/.worktrees/feature-x-n1");
});

it("shows no worktree indicator when the branch has no worktree on disk", () => {
  // Given — n1's branch resolves with no worktree
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    resolutionByBranch: { "feature/x/n1": { branch: "feature/x/n1", worktree: { exists: false } } },
  });

  // Then
  prStackScreenPage.worktree("n1").should("not.exist");
});

// ---------------------------------------------------------------------------
// In-progress session resolution (by branch, via QueryBranch)
// ---------------------------------------------------------------------------

it("marks a row in progress when QueryBranch resolves an active session for its branch", () => {
  // Given — n1's branch resolves to an active child session
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    resolutionByBranch: {
      "feature/x/n1": {
        branch: "feature/x/n1",
        session: { exists: true, sessionId: "child-n1", isActive: true, status: "active" },
      },
    },
  });

  // Then
  prStackScreenPage.inProgressBadge("n1").should("exist");
});

it("shows no in-progress indicator when QueryBranch resolves no session for the branch", () => {
  // Given — n1's branch resolves to no session
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    resolutionByBranch: { "feature/x/n1": { branch: "feature/x/n1", session: { exists: false } } },
  });

  // Then
  prStackScreenPage.inProgressBadge("n1").should("not.exist");
});

// ---------------------------------------------------------------------------
// GitHub PR status (from the same resolution)
// ---------------------------------------------------------------------------

it("shows the GitHub PR number as a link resolved by QueryBranch", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    resolutionByBranch: {
      "feature/x/n1": {
        branch: "feature/x/n1",
        pr: { exists: true, number: 42, url: "https://github.com/acme/repo/pull/42", state: "open" },
      },
    },
  });

  // Then
  prStackScreenPage
    .prLink("n1")
    .should("contain.text", "#42")
    .and("have.attr", "href", "https://github.com/acme/repo/pull/42");
});

it("updates the resolution on the polling interval without user action", () => {
  // Given — the branch has no worktree on the first poll and one on every subsequent poll
  cy.clock();
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);
  let polls = 0;
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    resolutionFactory: (branch) => {
      polls += 1;
      return {
        branch,
        worktree:
          polls === 1
            ? { exists: false }
            : { exists: true, path: "/home/dev/pr-stack-project/.worktrees/feature-x-n1" },
      };
    },
  });

  // Then — the first poll shows no worktree
  prStackScreenPage.worktree("n1").should("not.exist");

  // When — the poll interval elapses
  cy.tick(POLL_INTERVAL_MS);

  // Then — the row reflects the newer poll without any user action
  prStackScreenPage
    .worktree("n1")
    .should("contain.text", "/home/dev/pr-stack-project/.worktrees/feature-x-n1");
});

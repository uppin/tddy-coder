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
import { type SessionEntry } from "tddy-client-web/gen/session_pb";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { openProjectStackView } from "../support/rpc/openProjectStackView";
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

/** The interval (ms) at which the PR-Stack view re-polls branch resolution. */
const POLL_INTERVAL_MS = 5000;

interface MountOptions {
  /** The stack's plan, as `GetWorkQueue` answers it. */
  plan: string;
  /** The sessions the drawer lists — the children a planned node resolves to. */
  sessions?: Partial<SessionEntry>[];
  /** Static `QueryBranch` resolution per branch. */
  resolutionByBranch?: Record<string, BranchResolutionFixture>;
  /** Dynamic `QueryBranch` resolution per branch, re-evaluated on every poll. */
  resolutionFactory?: (branch: string) => BranchResolutionFixture;
}

function openStackView(opts: MountOptions) {
  const backend = aSessionsDrawerBackend(opts.sessions ?? []).onUnary(
    WorkQueueService.method.queryBranch,
    (req: { branch: string }) => {
      const fx = opts.resolutionFactory
        ? opts.resolutionFactory(req.branch)
        : (opts.resolutionByBranch?.[req.branch] ?? { branch: req.branch });
      return aBranchResolutionResponse(fx);
    },
  );
  openProjectStackView(backend, opts.plan, ORCHESTRATOR_SESSION_ID);
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
  openStackView({
    plan,
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
  openStackView({
    plan,
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
  openStackView({
    plan,
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
  openStackView({
    plan,
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
  openStackView({
    plan,
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
  openStackView({
    plan,
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

  // Then — the first poll has been answered, and shows no worktree. The row alone does not prove it:
  // the row renders from the stack before the poll starts, and the poll's interval is registered with
  // it, so a tick before the first poll would advance a clock nothing is scheduled on yet.
  prStackScreenPage.workItemRow("n1").should("exist");
  cy.wrap(null).should(() => expect(polls, "QueryBranch polls answered").to.be.greaterThan(0));
  prStackScreenPage.worktree("n1").should("not.exist");

  // When — the poll interval elapses
  cy.tick(POLL_INTERVAL_MS);

  // Then — the row reflects the newer poll without any user action
  prStackScreenPage
    .worktree("n1")
    .should("contain.text", "/home/dev/pr-stack-project/.worktrees/feature-x-n1");
});

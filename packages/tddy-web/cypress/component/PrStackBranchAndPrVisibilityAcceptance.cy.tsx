/**
 * Acceptance tests: a planned PR row makes its branch and its GitHub PR legible.
 *
 * The row rendered a worktree path and a PR link but never the branch itself, so an operator could
 * not tell which branch a planned PR owned. And a PR lookup that could not be performed came back
 * as `exists = false` — indistinguishable from "this branch has no PR" — which is why a live open PR
 * stayed invisible. The row now renders the owned branch, renders a planned branch *name* distinctly
 * from an owned one, and says so when the lookup itself was unavailable.
 *
 * A stub / demo login is deliberately *not* unavailable: it resolves to no PRs, exactly like a
 * repository with none (PRD D12).
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md (C3, D8, D12).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-8888-0000-0000-0000-000000000080";
const PROJECT_ID = "proj-pr-stack";
const OWNED_BRANCH = "feature/attach-docs/attach-proto";
const PLANNED_BRANCH = "feature/attach-docs/attach-start";

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
// Branch name
// ---------------------------------------------------------------------------

it("renders the branch a planned PR owns", () => {
  // Given
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Attachment proto", branch: OWNED_BRANCH }),
  ];

  // When
  openPrStackScreen(nodes, {});

  // Then
  prStackScreenPage.branchName("n1").should("contain.text", OWNED_BRANCH);
});

it("renders a planned branch name distinctly from a branch the node owns", () => {
  // Given — the node holds only a suggestion; no branch exists for it yet
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Copy attachments", branchSuggestion: PLANNED_BRANCH }),
  ];

  // When
  openPrStackScreen(nodes, {});

  // Then — a suggestion names no ref, so it must not read as an existing branch
  prStackScreenPage.plannedBranchName("n1").should("contain.text", PLANNED_BRANCH);
  prStackScreenPage.branchName("n1").should("not.exist");
});

// ---------------------------------------------------------------------------
// PR status: found / none / unavailable
// ---------------------------------------------------------------------------

it("shows the PR number and its state for a branch that has a pull request", () => {
  // Given
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Attachment proto", branch: OWNED_BRANCH }),
  ];

  // When
  openPrStackScreen(nodes, {
    [OWNED_BRANCH]: {
      branch: OWNED_BRANCH,
      pr: {
        exists: true,
        number: 351,
        url: "https://github.com/uppin/tddy-coder/pull/351",
        state: "open",
      },
    },
  });

  // Then
  prStackScreenPage
    .prLink("n1")
    .should("contain.text", "#351")
    .and("have.attr", "href", "https://github.com/uppin/tddy-coder/pull/351");
  prStackScreenPage.prState("n1").should("contain.text", "open");
});

it("shows no PR indicator when the branch genuinely has no pull request", () => {
  // Given — the lookup succeeded and found nothing. This is also the stub/demo-login shape (D12).
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Attachment proto", branch: OWNED_BRANCH }),
  ];

  // When
  openPrStackScreen(nodes, {
    [OWNED_BRANCH]: {
      branch: OWNED_BRANCH,
      pr: { exists: false, unavailable: false },
    },
  });

  // Then
  prStackScreenPage.prLink("n1").should("not.exist");
  prStackScreenPage.prUnavailable("n1").should("not.exist");
});

it("reports PR status as unavailable when the lookup could not be performed", () => {
  // Given — a real credential that could not be used, distinct from "no PR exists"
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Attachment proto", branch: OWNED_BRANCH }),
  ];

  // When
  openPrStackScreen(nodes, {
    [OWNED_BRANCH]: {
      branch: OWNED_BRANCH,
      pr: {
        exists: false,
        unavailable: true,
        unavailableReason: "GitHub token is missing the repo scope",
      },
    },
  });

  // Then — the operator learns the status is unknown, not that no PR exists
  prStackScreenPage.prUnavailable("n1").should("exist");
  prStackScreenPage.prLink("n1").should("not.exist");
});

it("carries the unavailability reason on the indicator", () => {
  // Given
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Attachment proto", branch: OWNED_BRANCH }),
  ];

  // When
  openPrStackScreen(nodes, {
    [OWNED_BRANCH]: {
      branch: OWNED_BRANCH,
      pr: {
        exists: false,
        unavailable: true,
        unavailableReason: "GitHub token is missing the repo scope",
      },
    },
  });

  // Then
  prStackScreenPage
    .prUnavailable("n1")
    .should("have.attr", "title", "GitHub token is missing the repo scope");
});

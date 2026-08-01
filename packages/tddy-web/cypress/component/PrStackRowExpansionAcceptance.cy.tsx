/**
 * Acceptance tests: a planned-PR row collapses to a summary and expands to its full detail.
 *
 * The row used to render every field it had unconditionally — title, description, owned branch,
 * planned branch, base branch and worktree path as six stacked lines — which in the panel's 360px
 * dock makes a five-node stack a wall of text with no hierarchy, and leaves nowhere to put the
 * fields the node genuinely has but the row never showed (node id, parents, child recipe).
 *
 * Collapsing is not suppression. Every field stays in the row, one interaction away, and the states
 * that demand attention — the status chip, the call to action, blockers and refusals — stay in the
 * always-visible region. The detail is hidden rather than unmounted, so a row keeps its expansion
 * across a branch-resolution poll tick and across the panel closing and reopening.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Panel UX (C1, D21–D22; AC 1–5).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7100-0000-0000-0000-000000000071";
const PROJECT_ID = "proj-pr-stack";
const POLL_INTERVAL_MS = 5000;

const ROOT_BRANCH = "feature/attach-docs/attach-proto";
const CHILD_BRANCH = "feature/attach-docs/attach-store";
const CHILD_WORKTREE_PATH = "/home/dev/worktrees/attach-store";

/** A two-node chain: a materialized root and the node stacked on it. */
function aTwoNodeStack(): StackNodeFixture[] {
  return [
    aPlannedNode({
      nodeId: "n1",
      title: "Attachment staging proto",
      description: "Proto, daemon handler and tests for staging an attachment.",
      branch: ROOT_BRANCH,
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Session attachment storage",
      description: "The on-disk store behind the staging RPC.",
      branch: CHILD_BRANCH,
      parents: ["n1"],
      childRecipe: "tdd-small",
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
  mainBranchRef: "origin/master",
  daemonInstanceId: "local",
};

interface MountOptions {
  nodes?: StackNodeFixture[];
  /** `QueryBranch` resolution per branch, consulted afresh on every poll. */
  resolutionByBranch?: Record<string, BranchResolutionFixture>;
}

function openPrStackScreen(opts: MountOptions = {}) {
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, opts.nodes ?? aTwoNodeStack())),
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(
        opts.resolutionByBranch?.[req.branch] ?? { branch: req.branch },
      ),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "opus", label: "Claude Opus (latest)" }],
      defaultModel: "opus",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: [] }));

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
// Collapsed: a summary, not a wall
// ---------------------------------------------------------------------------

it("collapses a planned-PR row to its title and its call to action", () => {
  // Given / When
  openPrStackScreen();

  // Then — the row identifies itself and offers its action, and nothing else competes for the space
  prStackScreenPage.rowToggle("n2").should("contain.text", "Session attachment storage");
  prStackScreenPage.startSessionBtn("n2").should("be.visible");
  prStackScreenPage.rowDetails("n2").should("not.be.visible");
});

it("keeps every detail line out of sight while a row is collapsed", () => {
  // Given — a node whose branch has a worktree, so every detail line has something to render
  // When
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        worktree: { exists: true, path: CHILD_WORKTREE_PATH },
      },
    },
  });

  // Then
  prStackScreenPage.branchName("n2").should("not.be.visible");
  prStackScreenPage.baseBranch("n2").should("not.be.visible");
  prStackScreenPage.worktree("n2").should("not.be.visible");
  prStackScreenPage.nodeIdLabel("n2").should("not.be.visible");
});

// ---------------------------------------------------------------------------
// Expanded: the row's full information
// ---------------------------------------------------------------------------

it("reveals the planned PR's full detail when its header is clicked", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        worktree: { exists: true, path: CHILD_WORKTREE_PATH },
      },
    },
  });

  // When
  prStackScreenPage.expandRow("n2");

  // Then — everything the node knows, including the fields the flat row never had room for
  prStackScreenPage.rowDetails("n2").should("be.visible");
  prStackScreenPage.branchName("n2").should("contain.text", CHILD_BRANCH);
  prStackScreenPage.baseBranch("n2").should("contain.text", ROOT_BRANCH);
  prStackScreenPage.worktree("n2").should("contain.text", CHILD_WORKTREE_PATH);
  prStackScreenPage.nodeIdLabel("n2").should("contain.text", "n2");
  prStackScreenPage.childRecipe("n2").should("contain.text", "tdd-small");
});

it("names the planned PRs a row is stacked on by their titles", () => {
  // Given — parents are recorded as node ids, which say nothing to an operator reading the panel
  // When
  openPrStackScreen();
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.parents("n2").should("contain.text", "Attachment staging proto");
});

it("collapses the row again when its header is clicked a second time", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.rowDetails("n2").should("be.visible");

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.rowDetails("n2").should("not.be.visible");
});

it("marks a row header as expanded for assistive technology", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.rowToggle("n2").should("have.attr", "aria-expanded", "false");

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.rowToggle("n2").should("have.attr", "aria-expanded", "true");
});

it("names the detail body a row header reveals for assistive technology", () => {
  // Given / When — `aria-expanded` says a region opened; only `aria-controls` says which one
  openPrStackScreen();

  // Then
  prStackScreenPage.expectRowToggleControlsDetails("n2");
});

it("expands only the row whose header was clicked", () => {
  // Given
  openPrStackScreen();

  // When
  prStackScreenPage.expandRow("n2");

  // Then — expansion is per row, not a mode the whole list enters
  prStackScreenPage.rowDetails("n2").should("be.visible");
  prStackScreenPage.rowDetails("n1").should("not.be.visible");
});

// ---------------------------------------------------------------------------
// Expansion outlives the things that re-render the row
// ---------------------------------------------------------------------------

it("keeps a row expanded when its branch resolution changes on the poll interval", () => {
  // Given — the worktree only appears on the second poll, so the row genuinely re-renders
  cy.clock();
  let pollCount = 0;
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, aTwoNodeStack())),
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) => {
      pollCount += 1;
      return aBranchResolutionResponse({
        branch: req.branch,
        worktree:
          pollCount > 2 && req.branch === CHILD_BRANCH
            ? { exists: true, path: CHILD_WORKTREE_PATH }
            : { exists: false },
      });
    })
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));

  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.expandRow("n2");

  // When
  cy.tick(POLL_INTERVAL_MS);

  // Then — the new value arrives without the row folding shut under the operator
  prStackScreenPage.worktree("n2").should("contain.text", CHILD_WORKTREE_PATH);
  prStackScreenPage.rowDetails("n2").should("be.visible");
});

it("keeps a row expanded after the Planned PRs panel is closed and reopened", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.togglePlannedPrPanel();
  prStackScreenPage.togglePlannedPrPanel();

  // Then — the panel hides rather than unmounts, and so does the row's detail
  prStackScreenPage.rowDetails("n2").should("be.visible");
});

// ---------------------------------------------------------------------------
// The call to action is not behind the toggle
// ---------------------------------------------------------------------------

it("starts a session from a collapsed row without expanding it", () => {
  // Given — the CTA sits beside the toggle, never nested inside it: a button within a button is
  // invalid markup and would swallow this click into an expand.
  openPrStackScreen();

  // When
  prStackScreenPage.startSessionBtn("n2").click();

  // Then
  prStackScreenPage.createSessionDialog().should("exist");
  prStackScreenPage.rowDetails("n2").should("not.be.visible");
});

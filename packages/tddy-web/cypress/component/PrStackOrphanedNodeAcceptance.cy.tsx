/**
 * Acceptance tests: a planned PR whose recorded child session no longer exists is recoverable.
 *
 * Deleting a child session leaves its id on the orchestrator's stack node — `DeleteSession` never
 * touches `Changeset.stack` — so a row keyed on `session_id` alone shows a status chip forever and
 * the planned PR becomes unworkable. The row instead treats a node as *orphaned* once its branch
 * resolution has arrived reporting no session, offers "Start session" again, and pre-fills the
 * dialog to **resume the branch the node already owns** rather than create a new one.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md (C1, D1–D3).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-6666-0000-0000-0000-000000000060";
const PROJECT_ID = "proj-pr-stack";
/** The branch the planned node already owns — created, pushed, and outliving its session. */
const OWNED_BRANCH = "feature/attach-docs/attach-store";
/**
 * The same branch as `ListProjectBranches` names it. That RPC lists remote-tracking refs
 * (`list_recent_remote_branches` reads `refs/remotes/origin`), so every option in the dialog's
 * branch picker is `origin/`-prefixed while a stack node records the unprefixed name.
 */
const OWNED_REMOTE_BRANCH = `origin/${OWNED_BRANCH}`;
/** The child session the node still records, whose session directory has been deleted. */
const DELETED_CHILD_SESSION_ID = "child-session-since-deleted";

/** A node that was spawned once: it owns a branch and still records its (now deleted) session. */
function aSpawnedNode() {
  return aPlannedNode({
    nodeId: "n1",
    title: "Session attachment storage",
    branch: OWNED_BRANCH,
    sessionId: DELETED_CHILD_SESSION_ID,
  });
}

function anOrchestratorSession(stackPlanJson: string): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-26T09:50:00Z",
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
  /** `QueryBranch` resolution per branch. */
  resolutionByBranch: Record<string, BranchResolutionFixture>;
  /** Branches `ListProjectBranches` offers for the "Work on existing branch" mode. */
  remoteBranches?: string[];
  /** The orchestrator's planned stack. Defaults to the single orphaned node above. */
  nodes?: StackNodeFixture[];
}

function openPrStackScreen(opts: MountOptions) {
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, opts.nodes ?? [aSpawnedNode()])),
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(opts.resolutionByBranch[req.branch] ?? { branch: req.branch }),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "opus", label: "Claude Opus (latest)" }],
      defaultModel: "opus",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: opts.remoteBranches ?? [],
    }));

  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
}

/** Open the screen with a `QueryBranch` call that never answers — the resolution never arrives. */
function openPrStackScreenWithUnansweredResolution() {
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, [aSpawnedNode()])),
  ]).onUnary(
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
// Orphan detection
// ---------------------------------------------------------------------------

it("offers Start session again for a node whose recorded child session no longer exists", () => {
  // Given — the node records a child session, and its branch resolves to no session at all
  // When
  openPrStackScreen({
    resolutionByBranch: {
      [OWNED_BRANCH]: { branch: OWNED_BRANCH, session: { exists: false } },
    },
  });

  // Then — the row is workable again instead of stuck on a status chip
  prStackScreenPage.startSessionBtn("n1").should("exist");
  prStackScreenPage.statusChip("n1").should("not.exist");
});

it("keeps the status chip for a node whose branch still resolves to a live session", () => {
  // Given — the recorded session is alive and owns the branch
  // When
  openPrStackScreen({
    resolutionByBranch: {
      [OWNED_BRANCH]: {
        branch: OWNED_BRANCH,
        session: {
          exists: true,
          sessionId: DELETED_CHILD_SESSION_ID,
          isActive: true,
          status: "active",
        },
      },
    },
  });

  // Then — a live child must never be mistaken for an orphan
  prStackScreenPage.statusChip("n1").should("exist");
  prStackScreenPage.startSessionBtn("n1").should("not.exist");
});

it("keeps the status chip while the branch resolution has not arrived yet", () => {
  // Given / When — QueryBranch never answers, so nothing is known about the recorded session
  openPrStackScreenWithUnansweredResolution();

  // Then — an unanswered poll is "unknown", never "orphaned"
  prStackScreenPage.statusChip("n1").should("exist");
  prStackScreenPage.startSessionBtn("n1").should("not.exist");
});

// ---------------------------------------------------------------------------
// Recovery: resume the branch the node already owns
// ---------------------------------------------------------------------------

it("pre-fills the dialog to work on the branch an orphaned node already owns", () => {
  // Given — an orphaned node owning a branch the project offers, listed after an unrelated one so the
  // picker's own "select the first branch" default cannot be mistaken for the pre-fill
  openPrStackScreen({
    resolutionByBranch: {
      [OWNED_BRANCH]: { branch: OWNED_BRANCH, session: { exists: false } },
    },
    remoteBranches: ["origin/master", OWNED_REMOTE_BRANCH],
  });

  // When
  prStackScreenPage.startSessionBtn("n1").click();

  // Then — resuming the existing branch, not creating one that already exists
  prStackScreenPage.dialogBranchIntentSelect().should("have.value", "work_on_selected_branch");
  prStackScreenPage.dialogBranchToWorkOnSelect().should("have.value", OWNED_REMOTE_BRANCH);
});

// ---------------------------------------------------------------------------
// Recovery is not gated on a base branch a resume never resolves
// ---------------------------------------------------------------------------

it("offers Start session for an orphaned node whose base branch is missing from the remote", () => {
  // Given — the orphan owns a pushed branch, but its predecessor owns none, so the node has no base.
  // A resume creates no branch and fetches no base (`work_on_selected_branch` skips chain-base
  // resolution entirely), so the base is irrelevant to whether this spawn can succeed.
  const nodes = [
    aPlannedNode({
      nodeId: "n0",
      title: "Start-session attachment proto",
      branchSuggestion: "feature/attach-docs/attach-proto",
    }),
    aPlannedNode({
      nodeId: "n1",
      title: "Session attachment storage",
      branch: OWNED_BRANCH,
      sessionId: DELETED_CHILD_SESSION_ID,
      parents: ["n0"],
    }),
  ];

  // When
  openPrStackScreen({
    nodes,
    resolutionByBranch: {
      [OWNED_BRANCH]: { branch: OWNED_BRANCH, session: { exists: false } },
    },
  });

  // Then — blocking here would be an unrecoverable dead end for work that is not lost at all
  prStackScreenPage.startSessionBtn("n1").should("be.enabled");
  prStackScreenPage.startWarning("n1").should("not.exist");
});

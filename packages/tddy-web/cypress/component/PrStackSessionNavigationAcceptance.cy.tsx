/**
 * Acceptance tests: a spawned planned PR's indicator opens the child session it is bound to.
 *
 * Once a node is spawned its call-to-action slot collapses to a plain status chip. The node records
 * `session_id`, and `QueryBranch` resolves the session that owns the branch, but neither was ever a
 * link: the operator read "building" and then hunted for the matching row in the session drawer by
 * eye. The chip now selects and attaches that session, exactly as clicking it in the drawer does.
 *
 * The bound session is resolved from the **plan** first and the branch's current owner second. The
 * chip is the node's recorded binding and the plan is the durable record; "who owns this branch
 * right now" is a different question whose answer changes after a resume or a hand-off. Both legs
 * are guarded on the session actually being known, so the control never selects nothing.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md (C2, D23; AC 6–7).
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7200-0000-0000-0000-000000000072";
const PROJECT_ID = "proj-pr-stack";
const OWNED_BRANCH = "feature/attach-docs/attach-store";

/** The child session the plan records for the node. */
const RECORDED_CHILD_SESSION_ID = "child-session-recorded-by-the-plan";
/** A different session that currently owns the branch — e.g. after a resume. */
const BRANCH_OWNER_SESSION_ID = "child-session-that-owns-the-branch-now";

function aSpawnedNode(sessionId: string): StackNodeFixture {
  return aPlannedNode({
    nodeId: "n1",
    title: "Session attachment storage",
    branch: OWNED_BRANCH,
    sessionId,
    childState: "Implementing",
  });
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

/** A spawned child. `claude-cli` runs a terminal, so selecting it replaces the PR-Stack screen. */
function aChildSession(sessionId: string): Partial<SessionEntry> {
  return {
    sessionId,
    createdAt: "2026-08-01T09:30:00Z",
    status: "active",
    repoPath: "/home/dev/pr-stack-project",
    isActive: true,
    projectId: PROJECT_ID,
    sessionType: "claude-cli",
    branch: OWNED_BRANCH,
    orchestratorSessionId: ORCHESTRATOR_SESSION_ID,
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
  /** The session id the plan records on the node. */
  recordedSessionId: string;
  /** Sessions the drawer knows about, besides the orchestrator. */
  childSessions: Partial<SessionEntry>[];
  /** `QueryBranch` resolution per branch. */
  resolutionByBranch?: Record<string, BranchResolutionFixture>;
}

function openPrStackScreen(opts: MountOptions) {
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, [aSpawnedNode(opts.recordedSessionId)])),
    ...opts.childSessions,
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(
        opts.resolutionByBranch?.[req.branch] ?? { branch: req.branch },
      ),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));

  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
}

/** A resolution reporting that a live session owns the branch. */
function ownedBy(sessionId: string): BranchResolutionFixture {
  return {
    branch: OWNED_BRANCH,
    session: { exists: true, sessionId, isActive: true, status: "active" },
  };
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
// Opening the bound session
// ---------------------------------------------------------------------------

it("opens the child session a spawned planned PR is bound to", () => {
  // Given
  openPrStackScreen({
    recordedSessionId: RECORDED_CHILD_SESSION_ID,
    childSessions: [aChildSession(RECORDED_CHILD_SESSION_ID)],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy(RECORDED_CHILD_SESSION_ID) },
  });

  // When
  prStackScreenPage.openBoundSession("n1");

  // Then — the app navigated: the child is selected, and its own view replaced the PR-Stack screen
  sessionsDrawerPage.expectSessionSelected(RECORDED_CHILD_SESSION_ID);
  prStackScreenPage.screen().should("not.exist");
});

it("shows the bound session's id in the row's detail", () => {
  // Given
  openPrStackScreen({
    recordedSessionId: RECORDED_CHILD_SESSION_ID,
    childSessions: [aChildSession(RECORDED_CHILD_SESSION_ID)],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy(RECORDED_CHILD_SESSION_ID) },
  });

  // When
  prStackScreenPage.expandRow("n1");

  // Then
  prStackScreenPage.childSession("n1").should("contain.text", RECORDED_CHILD_SESSION_ID);
});

it("keeps the status chip readable inside the control that opens the session", () => {
  // Given — the chip is wrapped, not replaced: its own contract is unchanged
  // When
  openPrStackScreen({
    recordedSessionId: RECORDED_CHILD_SESSION_ID,
    childSessions: [aChildSession(RECORDED_CHILD_SESSION_ID)],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy(RECORDED_CHILD_SESSION_ID) },
  });

  // Then
  prStackScreenPage.statusChip("n1").should("contain.text", "Implementing");
});

it("names what the control does and which session it opens, for assistive technology", () => {
  // Given — the control's content is the chip's phase text, so without a stated name a screen
  // reader announces "Implementing, button" and nothing about where it goes
  // When
  openPrStackScreen({
    recordedSessionId: RECORDED_CHILD_SESSION_ID,
    childSessions: [aChildSession(RECORDED_CHILD_SESSION_ID)],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy(RECORDED_CHILD_SESSION_ID) },
  });

  // Then
  prStackScreenPage
    .sessionRef("n1")
    .should(
      "have.attr",
      "aria-label",
      `Open child session ${RECORDED_CHILD_SESSION_ID} for Session attachment storage`,
    );
});

// ---------------------------------------------------------------------------
// Which session the row binds to
// ---------------------------------------------------------------------------

it("binds to the session that owns the branch when the recorded child is not a known session", () => {
  // Given — the plan names a session the drawer has never heard of (deleted on another host, or the
  // branch was picked up by a fresh session), while a real session owns the branch right now
  openPrStackScreen({
    recordedSessionId: "child-session-no-host-reports",
    childSessions: [aChildSession(BRANCH_OWNER_SESSION_ID)],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy(BRANCH_OWNER_SESSION_ID) },
  });

  // When
  prStackScreenPage.openBoundSession("n1");

  // Then — falling back beats offering a link that would select nothing
  sessionsDrawerPage.expectSessionSelected(BRANCH_OWNER_SESSION_ID);
});

it("binds to the session the plan records even when another session owns the branch", () => {
  // Given — both resolve, and they differ: the plan's binding is the durable record and wins
  openPrStackScreen({
    recordedSessionId: RECORDED_CHILD_SESSION_ID,
    childSessions: [
      aChildSession(RECORDED_CHILD_SESSION_ID),
      aChildSession(BRANCH_OWNER_SESSION_ID),
    ],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy(BRANCH_OWNER_SESSION_ID) },
  });

  // When
  prStackScreenPage.openBoundSession("n1");

  // Then
  sessionsDrawerPage.expectSessionSelected(RECORDED_CHILD_SESSION_ID);
});

it("renders the status chip as plain text when no bound session can be resolved", () => {
  // Given — a live session owns the branch, so the node is not orphaned and the row keeps its chip,
  // but neither the session the plan records nor the one owning the branch is a session the drawer
  // knows: both were spawned on a host that is not reporting.
  openPrStackScreen({
    recordedSessionId: "child-session-no-host-reports",
    childSessions: [],
    resolutionByBranch: { [OWNED_BRANCH]: ownedBy("another-session-no-host-reports") },
  });

  // Then — a control that would select nothing is worse than no control
  prStackScreenPage.statusChip("n1").should("exist");
  prStackScreenPage.sessionRef("n1").should("not.exist");
});

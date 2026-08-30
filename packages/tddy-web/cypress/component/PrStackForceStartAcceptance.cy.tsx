/**
 * Acceptance tests: a blocked planned PR can still be started — the blockers advise, they no longer
 * refuse.
 *
 * D16 kept a blocked row's full information and left its Start-session button in place, disabled,
 * on the reasoning that Repoint is what re-enables it. That held while the blockers were true. Two
 * of the three are now known to be derivable from **local-only false negatives**: `remote.exists` is
 * read from the queried daemon's own remote-tracking refs, so a branch pushed from another host reads
 * absent until this clone fetches, and `parent-has-no-branch` is true for every parent whose node link
 * was written on the wrong host. A gate that cannot see half the fleet must advise, not refuse.
 *
 * So the button stays pressable and takes a warning colour with an alert icon; the row keeps the
 * warning box naming every reason (D22), and the tooltip repeats them on the control itself. The
 * daemon still enforces its own spawn gate, so a genuinely impossible spawn fails there with the real
 * reason — strictly more information than a button that cannot be pressed.
 *
 * No extra confirmation step (D43): `CreateSessionDialog` already stands between the click and the
 * spawn, showing the base branch, the branch name and the prompt the child will get.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D42, D43), amending D16.
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-9300-0000-0000-0000-000000000093";
const PROJECT_ID = "proj-pr-stack";
/** The predecessor's branch — the base `n2` would be branched from. */
const BASE_BRANCH = "feature/attach-docs/attach-proto";

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: "origin/master",
  daemonInstanceId: "local",
};

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

/**
 * `n3` is blocked twice over: one parent owns no branch, and the branch the other one owns — which
 * is what its base resolves to — is absent from this host's `origin`.
 */
const TWO_BLOCKERS_AT_ONCE: StackNodeFixture[] = [
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
    branchSuggestion: "feature/attach-docs/attach-copy",
    parents: ["n1", "n2"],
  }),
];

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
 * Open the PR-Stack screen and hand back the backend, so a scenario that presses through the
 * create-session dialog can read the `StartSession` it submitted.
 *
 * The catalogs `CreateSessionPane` loads on mount are stubbed here rather than per scenario: the
 * dialog is the review step every force start goes through (D43), so a scenario that cannot reach
 * Create cannot state what a force start actually sends.
 */
function openPrStackScreen(
  nodes: StackNodeFixture[],
  resolutionByBranch: Record<string, BranchResolutionFixture>,
) {
  const backend = aSessionsDrawerBackend([anOrchestratorSession(nodes)])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(resolutionByBranch[req.branch] ?? { branch: req.branch }),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", name: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: [],
      defaultRemote: "origin",
    }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: "child-forced-start-1",
      livekitRoom: "room-child-forced-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }));

  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return backend;
}

/** The one `StartSession` the dialog submitted. */
function theSubmittedStart(
  backend: ReturnType<typeof openPrStackScreen>,
  assertion: (call: { selectedIntegrationBaseRef: string }) => void,
) {
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    assertion(calls[0] as { selectedIntegrationBaseRef: string });
  });
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
// A blocked node keeps a pressable button
// ---------------------------------------------------------------------------

it("marks a blocked Start session with a warning colour and an alert icon", () => {
  // Given
  // When
  openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: { branch: BASE_BRANCH, remote: { exists: false } },
  });

  // Then — pressable, but never mistakable for an ordinary start
  prStackScreenPage.startSessionBtn("n2").should("have.attr", "data-variant", "warning");
  prStackScreenPage.startSessionBlockedIcon("n2").should("exist");
});

it("names every blocker on the pressable button", () => {
  // Given — n3 is blocked by a branchless parent *and* by a base branch absent from origin
  // When
  openPrStackScreen(TWO_BLOCKERS_AT_ONCE, {
    [BASE_BRANCH]: { branch: BASE_BRANCH, remote: { exists: false } },
  });

  // Then — the tooltip repeats what the row's warning box says, on a control that still works
  prStackScreenPage
    .startSessionBtn("n3")
    .should(
      "have.attr",
      "title",
      `Session attachment storage has not created its branch yet; Base branch ${BASE_BRANCH} is not on origin`,
    );
  prStackScreenPage
    .startWarning("n3")
    .should(
      "have.text",
      `Session attachment storage has not created its branch yetBase branch ${BASE_BRANCH} is not on origin`,
    );
});

it("opens the create-session dialog when a blocked node is started anyway", () => {
  // Given
  openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: { branch: BASE_BRANCH, remote: { exists: false } },
  });

  // When
  prStackScreenPage.startSessionBtn("n2").click();

  // Then — the dialog is the review step; a confirm in front of it would be a click, not a safeguard
  sessionsDrawerPage.createSessionDialog().should("exist");
});

// ---------------------------------------------------------------------------
// The daemon's gate decides, not the view's
// ---------------------------------------------------------------------------

it("sends no base branch of its own when a blocked node is started anyway", () => {
  // Given — the view believes n2's base is unusable, and it derived that belief from a leg that
  // cannot see another host
  const backend = openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: { branch: BASE_BRANCH, remote: { exists: false } },
  });

  // When the operator forces the start through
  prStackScreenPage.startSessionBtn("n2").click();
  prStackScreenPage.dialogSubmitBtn().click();

  // Then — an explicit `selected_integration_base_ref` takes precedence over chain resolution on the
  // daemon (`select_worktree_base_ref`), so sending one would cut the worktree from a base the view
  // guessed and skip the daemon's own gate. Force start hands the decision to that gate (D42): the
  // view must not pre-empt it with a base it could not resolve.
  theSubmittedStart(backend, (call) => {
    expect(call.selectedIntegrationBaseRef).to.equal("");
  });
});

it("still offers every base branch for a blocked node to be pointed at deliberately", () => {
  // Given
  openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: { branch: BASE_BRANCH, remote: { exists: false } },
  });

  // When
  prStackScreenPage.startSessionBtn("n2").click();

  // Then — only the *pre-selection* is dropped. A base the operator picks on purpose is a decision,
  // not a guess, and taking the picker away would leave a blocked node startable only one way.
  prStackScreenPage
    .dialogBaseBranchOptionValues()
    .should("deep.equal", [`origin/${BASE_BRANCH}`, "origin/master"]);
});

it("sends the derived base branch when nothing blocks the node", () => {
  // Given — n1's branch is on origin, so the base the view derived is one it could resolve
  const backend = openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: {
      branch: BASE_BRANCH,
      remote: { exists: true, sha: "4e2e8e8cf5de99f8485e518e925d382ae9275c76" },
    },
  });

  // When
  prStackScreenPage.startSessionBtn("n2").click();
  prStackScreenPage.dialogSubmitBtn().click();

  // Then — the chain base is still stated explicitly for an unblocked spawn, as the remote-tracking
  // ref the daemon fetches
  theSubmittedStart(backend, (call) => {
    expect(call.selectedIntegrationBaseRef).to.equal(`origin/${BASE_BRANCH}`);
  });
});

// ---------------------------------------------------------------------------
// An unblocked node is unchanged
// ---------------------------------------------------------------------------

it("renders a plain Start session button when nothing blocks the node", () => {
  // Given — n1's branch is on origin, so n2 has a base to be cut from
  // When
  openPrStackScreen(A_SPAWNED_PREDECESSOR_AND_ITS_DEPENDENT, {
    [BASE_BRANCH]: {
      branch: BASE_BRANCH,
      remote: { exists: true, sha: "4e2e8e8cf5de99f8485e518e925d382ae9275c76" },
    },
  });

  // Then
  prStackScreenPage.startSessionBtn("n2").should("have.attr", "data-variant", "default");
  prStackScreenPage.startSessionBlockedIcon("n2").should("not.exist");
  prStackScreenPage.startWarning("n2").should("not.exist");
});

/**
 * Acceptance: adding a planned PR and starting its session in one step.
 *
 * Adding a node and starting its session are the same intent in the common case — the operator wants
 * the next PR in the stack to exist and be worked on — but they were two controls in two places: the
 * "New planned PR" form, then hunting the new row for its own CTA. "Add & start session" is the same
 * `AddPlannedPr` call followed by the same Start-session dialog, opened on the node that call just
 * appended.
 *
 * Which node that is, the **response says**: `AddPlannedPrResponse.node_id` names the node the call
 * created. It cannot be inferred by diffing the returned plan against the one held before the call —
 * the orchestrator agent appends nodes to the same stack, and the screen's own view of it only
 * refreshes when the session list is refetched, so the plan that comes back can hold several ids this
 * screen has never seen. Any positional pick among them (the first, the last) can be the agent's node
 * rather than the operator's, and the dialog would then open pre-filled for somebody else's branch.
 *
 * "Add" on its own is unchanged — a node planned for later is still a node planned for later.
 *
 * Feature: docs/ft/coder/pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { ConnectionService } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { createSessionPage } from "../support/pages/createSessionPage";
import {
  aPlannedNode,
  aStackPlanJson,
  type StackNodeFixture,
} from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-4444-0000-0000-0000-000000000040";

/** The node the orchestrator already has: the stack's base, bound to a real branch. */
const BASE_NODE = aPlannedNode({
  nodeId: "n1",
  title: "Add token store",
  branch: "feat/auth-store",
  sessionId: "session-auth-store",
});

/** The branch suggestion the operator types into the form. */
const TYPED_BRANCH_SUGGESTION = "feature/auth/middleware";

/**
 * The branch suggestion the *added node* carries back.
 *
 * Deliberately NOT the string the operator typed: the daemon normalizes a suggestion that collides
 * with an existing branch, and the dialog must be pre-filled from the node the call appended rather
 * than from the form input that is still on screen. Make these two equal and the pre-fill assertion
 * passes either way — do not "tidy" them back together.
 */
const NORMALIZED_BRANCH_SUGGESTION = "feature/auth/middleware-2";

/** The base branch the added node's child is stacked onto, as the dialog submits it. */
const BASE_NODE_BASE_REF = "origin/feat/auth-store";

/** The node `AddPlannedPr` appends, as the daemon assigns it. */
const ADDED_NODE = aPlannedNode({
  nodeId: "n2",
  title: "Add auth middleware",
  branchSuggestion: NORMALIZED_BRANCH_SUGGESTION,
  parents: ["n1"],
});

function anOrchestratorSession(stackPlanJson: string) {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-13T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    pid: 0,
    isActive: false,
    projectId: "proj-pr-stack",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "pr-stack",
    stackPlanJson,
  };
}

/** How the daemon answers the add: the plan it re-read, and the node it says this call created. */
interface AddPlannedPrAnswer {
  /** The plan the response carries. Defaults to the base node plus the operator's added node. */
  nodes?: StackNodeFixture[];
  /** The `node_id` the response names as the one it appended. Defaults to the added node's. */
  createdNodeId?: string;
}

/**
 * The screen open on an orchestrator holding just the base node, with `AddPlannedPr` answering as
 * `answer` describes. The default is the plan grown by exactly one node, named as created — the quiet
 * case; a scenario overrides either half to describe a plan the screen cannot read positionally, or a
 * response whose two halves disagree.
 */
function openPrStackScreen(answer: AddPlannedPrAnswer = {}) {
  const nodes = answer.nodes ?? [BASE_NODE, ADDED_NODE];
  const nodeId = answer.createdNodeId ?? ADDED_NODE.nodeId;
  const backend = aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, [BASE_NODE])),
  ]).onUnary(ConnectionService.method.addPlannedPr, () => ({
    stackPlanJson: aStackPlanJson(1, nodes),
    nodeId,
  }));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return backend;
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
// Tests
// ---------------------------------------------------------------------------

it("offers an Add & start session action on the New planned PR form", () => {
  // Given
  openPrStackScreen();

  // When
  prStackScreenPage.openAddPlannedPrForm();

  // Then
  prStackScreenPage.addPlannedPrStartBtn().should("exist");
});

it("opens the start-session dialog for the node it just added", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.openAddPlannedPrForm();

  // When
  prStackScreenPage.fillAddPlannedPrFormAndStartSession({
    title: "Add auth middleware",
    branchSuggestion: TYPED_BRANCH_SUGGESTION,
    ancestorNodeIds: ["n1"],
  });

  // Then
  prStackScreenPage.createSessionDialog().should("be.visible");
});

it("still calls AddPlannedPr exactly once with the entered fields", () => {
  // Given
  const backend = openPrStackScreen();
  prStackScreenPage.openAddPlannedPrForm();

  // When
  prStackScreenPage.fillAddPlannedPrFormAndStartSession({
    title: "Add auth middleware",
    branchSuggestion: TYPED_BRANCH_SUGGESTION,
    ancestorNodeIds: ["n1"],
  });

  // Then the one-step action adds the node the same way "Add" does — it only continues further
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.addPlannedPr);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].title).to.equal("Add auth middleware");
    expect(calls[0].branchSuggestion).to.equal(TYPED_BRANCH_SUGGESTION);
    expect(calls[0].parents).to.deep.equal(["n1"]);
  });
});

it("pre-fills the start-session dialog with the added node's branch suggestion", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.openAddPlannedPrForm();

  // When the operator types one suggestion and the daemon answers with the normalized one
  prStackScreenPage.fillAddPlannedPrFormAndStartSession({
    title: "Add auth middleware",
    branchSuggestion: TYPED_BRANCH_SUGGESTION,
    ancestorNodeIds: ["n1"],
  });

  // Then the dialog is opened on the *added node*, so the branch it would create is the one that node
  // came back planning — not the value still sitting in the form input
  createSessionPage.newBranchNameInput().should("have.value", NORMALIZED_BRANCH_SUGGESTION);
});

it("bases the started session on the branch of the ancestor it was stacked on", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.openAddPlannedPrForm();

  // When
  prStackScreenPage.fillAddPlannedPrFormAndStartSession({
    title: "Add auth middleware",
    branchSuggestion: TYPED_BRANCH_SUGGESTION,
    ancestorNodeIds: ["n1"],
  });

  // Then the base the dialog will submit *is* the base node's branch — the stacking this whole flow
  // exists for. The selected value, not merely text somewhere in the dialog: the branch also appears
  // on the base node's own row behind it.
  prStackScreenPage.dialogBaseBranchSelect().should("have.value", BASE_NODE_BASE_REF);
});

/**
 * Inherent regression guard: nothing can currently start a session from "Add", so this cannot fail
 * before "Add & start session" exists. Its value is pinning that the new action does not leak into
 * the old one now that both are on the form.
 */
it("adds the node without opening the dialog when only Add is used", () => {
  // Given
  openPrStackScreen();
  prStackScreenPage.openAddPlannedPrForm();

  // When the operator plans a PR for later
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({
    title: "Add auth middleware",
    ancestorNodeIds: ["n1"],
  });

  // Then the node is on the list — the add landed — and nothing was started
  prStackScreenPage.plannedPrRow("n2").should("exist");
  prStackScreenPage.createSessionDialog().should("not.exist");
});

/**
 * **The defect this contract exists for.** The orchestrator agent appends nodes to the same stack, and
 * the screen's view of it refreshes only on the next session-list refetch — so the plan that comes back
 * can hold more than one node this screen has never seen, in any position. The operator's node is named
 * by the response, and nothing else identifies it.
 *
 * Both new nodes here are real races: the agent's `n2` was added while the form was open (so it is
 * *before* the operator's in the plan), and its `n4` landed between the daemon's write and the re-read
 * that serializes the response (so it is *after*). The operator's node is neither the first nor the
 * last unknown id — the two heuristics a diff can offer.
 */
it("opens the dialog on the node the server named, not another new node in the plan", () => {
  // Given a plan that came back with the agent's nodes surrounding the operator's
  const AGENT_NODE_BEFORE = aPlannedNode({
    nodeId: "n2",
    title: "Agent: split the token store",
    branchSuggestion: "feature/agent/token-store-split",
    parents: ["n1"],
  });
  const OPERATORS_NODE = aPlannedNode({
    nodeId: "n3",
    title: "Add auth middleware",
    branchSuggestion: NORMALIZED_BRANCH_SUGGESTION,
    parents: ["n1"],
  });
  const AGENT_NODE_AFTER = aPlannedNode({
    nodeId: "n4",
    title: "Agent: rotate the signing key",
    branchSuggestion: "feature/agent/rotate-signing-key",
    parents: ["n1"],
  });
  openPrStackScreen({
    nodes: [BASE_NODE, AGENT_NODE_BEFORE, OPERATORS_NODE, AGENT_NODE_AFTER],
    createdNodeId: OPERATORS_NODE.nodeId,
  });
  prStackScreenPage.openAddPlannedPrForm();

  // When the operator adds their PR and starts its session in one step
  prStackScreenPage.fillAddPlannedPrFormAndStartSession({
    title: "Add auth middleware",
    branchSuggestion: TYPED_BRANCH_SUGGESTION,
    ancestorNodeIds: ["n1"],
  });

  // Then the dialog is opened on the node the operator added — a session started for one of the
  // agent's nodes would create the wrong branch and bind it to a node nobody asked to work on
  createSessionPage.newBranchNameInput().should("have.value", NORMALIZED_BRANCH_SUGGESTION);
});

it("reports the failure and starts nothing when the named node is absent from the returned plan", () => {
  // Given a daemon whose response names a node its own plan does not contain — the two halves
  // disagree, so there is nothing trustworthy to open the dialog on, and guessing would start a
  // session for the wrong branch
  openPrStackScreen({ nodes: [BASE_NODE], createdNodeId: ADDED_NODE.nodeId });
  prStackScreenPage.openAddPlannedPrForm();

  // When
  prStackScreenPage.fillAddPlannedPrFormAndStartSession({
    title: "Add auth middleware",
    branchSuggestion: TYPED_BRANCH_SUGGESTION,
    ancestorNodeIds: ["n1"],
  });

  // Then the form says why and stays open, and no session-creation dialog appears
  prStackScreenPage
    .addPlannedPrError()
    .should(
      "have.text",
      "The added planned PR is missing from the returned stack plan — no session was started.",
    );
  prStackScreenPage.createSessionDialog().should("not.exist");
});

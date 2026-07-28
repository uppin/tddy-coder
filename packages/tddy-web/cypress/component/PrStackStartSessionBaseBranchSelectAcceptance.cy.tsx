/**
 * Acceptance tests: the PR-Stack "Start session" dialog lets the operator **choose** the base branch
 * for a planned-PR child session — a node with multiple non-merged parents (a diamond / merge node)
 * no longer silently bases off the first parent the resolver walks. The dialog renders a "Base branch"
 * `<select>` listing the node's direct dependency branches first (ordered by the dependency's own
 * depth in the stack DAG, deepest first, ties by `node.parents` order), then the stack's other
 * materialized branches. The selected value is sent as `StartSessionRequest.selected_integration_base_ref`.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-07-27-planned-pr-base-branch-selection.md.
 * Changeset: docs/dev/1-WIP/2026-07-27-planned-pr-base-branch-selection.md.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-basesel-0000-0000-0000-000000000070";
const PROJECT_ID = "proj-pr-stack";
const CHILD_SESSION_ID = "child-session-basesel-1";

// The project's resolved default remote — the daemon exposes it via `ProjectEntry.default_remote`,
// and the Start-session dialog lifts each base-branch option into the `<remote>/<branch>` ref the
// daemon fetches. A stack node's `branch` is a local name; the picker's options and the
// `selected_integration_base_ref` the dialog submits are remote-tracking refs.
const REMOTE = "origin";
const ATTACH_PROTO_BRANCH = "feature/session-attach-docs/attach-proto";
const ATTACH_STORE_BRANCH = "feature/session-attach-docs/attach-store";
const ATTACH_PROTO_REF = `${REMOTE}/${ATTACH_PROTO_BRANCH}`;
const ATTACH_STORE_REF = `${REMOTE}/${ATTACH_STORE_BRANCH}`;

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: "origin/master",
  defaultRemote: REMOTE,
  daemonInstanceId: "local",
};

/**
 * The orchestrator session from the live scenario: a diamond where `attach-start` depends on both
 * `attach-proto` and `attach-store` (both roots, both materialized).
 */
function anAttachDocsOrchestrator(): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-27T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, [
      aPlannedNode({
        nodeId: "attach-proto",
        title: "Start-session attachment proto",
        branch: ATTACH_PROTO_BRANCH,
        sessionId: "child-attach-proto",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "attach-store",
        title: "Session attachment storage and context docs",
        branch: ATTACH_STORE_BRANCH,
        sessionId: "child-attach-store",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "attach-start",
        title: "Copy attachments during StartSession",
        branchSuggestion: "feature/session-attach-docs/attach-start",
        parents: ["attach-proto", "attach-store"],
      }),
      aPlannedNode({
        nodeId: "attach-ui",
        title: "Create Session attach UI",
        branchSuggestion: "feature/session-attach-docs/attach-ui",
        parents: ["attach-start"],
      }),
    ]),
  };
}

/**
 * A backend seeded for the whole flow: the orchestrator session in the drawer, every RPC the reused
 * `CreateSessionPane` fetches on mount, and the StartSession the dialog submits.
 */
function aBaseSelectionBackend(orchestrator: Partial<SessionEntry>) {
  return aSessionsDrawerBackend([orchestrator])
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: CHILD_SESSION_ID,
      livekitRoom: "room-child-basesel-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }));
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

it("renders the base-branch selector with the diamond node's direct dependency branches in node.parents order", () => {
  // Given — the attach-start diamond: parents [attach-proto, attach-store], both roots (depth 0).
  const backend = aBaseSelectionBackend(anAttachDocsOrchestrator());

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("attach-start").click();

  // Then — the selector lists attach-proto first (first in node.parents), then attach-store, each
  // lifted to its `<remote>/<branch>` remote-tracking ref (the form the daemon fetches).
  prStackScreenPage.dialogBaseBranchSelect().should("be.visible");
  prStackScreenPage
    .dialogBaseBranchSelect()
    .find("option")
    .then(($opts) => [...$opts].map((o) => (o as HTMLOptionElement).value))
    .should("deep.equal", [ATTACH_PROTO_REF, ATTACH_STORE_REF]);
});

it("defaults the selection to the first direct dependency branch for the attach-start diamond", () => {
  // Given
  const backend = aBaseSelectionBackend(anAttachDocsOrchestrator());

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("attach-start").click();

  // Then — the default selection is attach-proto (first in node.parents), as a remote-tracking ref.
  prStackScreenPage.dialogBaseBranchSelect().should("have.value", ATTACH_PROTO_REF);
});

it("orders direct dependency branches by the dependency's own depth (deepest first) for a diamond", () => {
  // Given — a diamond where n3 depends on [n2, n1] and n2 depends on n1: n2 is depth 1, n1 is depth 0,
  // so n2 (the deeper, more specific base) is listed first.
  const orchestrator: Partial<SessionEntry> = {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-27T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, [
      aPlannedNode({
        nodeId: "n1",
        title: "root",
        branch: "feature/stack/n1",
        sessionId: "child-n1",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "n2",
        title: "mid",
        branch: "feature/stack/n2",
        sessionId: "child-n2",
        prStatus: { phase: "open" },
        parents: ["n1"],
      }),
      aPlannedNode({
        nodeId: "n3",
        title: "diamond top",
        branchSuggestion: "feature/stack/n3",
        // n2 is listed second in parents but is deeper, so it must come first in the selector.
        parents: ["n1", "n2"],
      }),
    ]),
  };
  const backend = aBaseSelectionBackend(orchestrator);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n3").click();

  // Then — n2 (depth 1) precedes n1 (depth 0), even though n1 is first in node.parents. Each option
  // is lifted to its `<remote>/<branch>` remote-tracking ref.
  prStackScreenPage
    .dialogBaseBranchSelect()
    .find("option")
    .then(($opts) => [...$opts].map((o) => (o as HTMLOptionElement).value))
    .should("deep.equal", [`${REMOTE}/feature/stack/n2`, `${REMOTE}/feature/stack/n1`]);
});

it("lists other materialized stack branches after the direct dependencies", () => {
  // Given — n3 depends on n1 only; n2 is a materialized sibling root that is NOT a direct dependency,
  // so it appears in the "other" section after n1.
  const orchestrator: Partial<SessionEntry> = {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-27T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, [
      aPlannedNode({
        nodeId: "n1",
        title: "dep",
        branch: "feature/stack/n1",
        sessionId: "child-n1",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "n2",
        title: "sibling",
        branch: "feature/stack/n2",
        sessionId: "child-n2",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "n3",
        title: "child",
        branchSuggestion: "feature/stack/n3",
        parents: ["n1"],
      }),
    ]),
  };
  const backend = aBaseSelectionBackend(orchestrator);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n3").click();

  // Then — n1 (direct dep) first, then n2 (other materialized branch), each as a remote-tracking ref.
  prStackScreenPage
    .dialogBaseBranchSelect()
    .find("option")
    .then(($opts) => [...$opts].map((o) => (o as HTMLOptionElement).value))
    .should("deep.equal", [`${REMOTE}/feature/stack/n1`, `${REMOTE}/feature/stack/n2`]);
});

it("sends the selected base branch as selected_integration_base_ref when the operator picks the non-default parent", () => {
  // Given — the attach-start diamond, defaulting to attach-proto.
  const backend = aBaseSelectionBackend(anAttachDocsOrchestrator());

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("attach-start").click();
  // The operator picks attach-store instead of the default attach-proto. The option value is the
  // `<remote>/<branch>` remote-tracking ref, which is what the dialog submits as
  // `selected_integration_base_ref` — the form the daemon fetches (`git fetch origin feature/...`).
  prStackScreenPage.dialogBaseBranchSelect().select(ATTACH_STORE_REF);
  prStackScreenPage.dialogSubmitBtn().click();

  // Then — StartSession carries attach-store as the integration base ref (remote-tracking), parented
  // to this orchestrator.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].stackParent).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].selectedIntegrationBaseRef).to.equal(ATTACH_STORE_REF);
  });
});

it("hides the base-branch selector for a root node with no other materialized branches and sends an empty base ref", () => {
  // Given — a single root node n1 (planned, no parents, no other materialized branches).
  const orchestrator: Partial<SessionEntry> = {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-27T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, [
      aPlannedNode({
        nodeId: "n1",
        title: "root",
        branchSuggestion: "feature/stack/n1",
      }),
    ]),
  };
  const backend = aBaseSelectionBackend(orchestrator);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();
  prStackScreenPage.dialogSubmitBtn().click();

  // Then — no selector is rendered, and StartSession sends an empty integration base ref (default base).
  prStackScreenPage.dialogBaseBranchSelect().should("not.exist");
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].selectedIntegrationBaseRef).to.equal("");
  });
});

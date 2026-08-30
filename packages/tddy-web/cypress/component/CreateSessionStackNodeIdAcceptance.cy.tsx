/**
 * Acceptance tests: the planned node a spawn materializes travels with the spawn, as
 * `StartSessionRequest.stack_node_id`.
 *
 * The daemon used to re-derive the node from the branch the spawn creates (`pr_stack_node_for_spawn`
 * matches on `new_branch_name`), which the operator can rename in this very form before confirming —
 * and which the spawning daemon cannot look up at all when the orchestrator lives on another host.
 * The node id is the one fact the surface that opened the form already knows exactly (D34).
 *
 * It is therefore not the operator's to edit here — but the **parent** is. The node id names a node
 * in the pre-filled parent's plan and in no other, so re-parenting the spawn has to drop it: the
 * daemon answers `LinkStackNode` for an id the new parent's plan does not hold with `not_found` and,
 * per D36, only logs it. The node would stay branchless and childless — the very bug the id exists
 * to fix — with nothing said to the operator. Dropping it instead falls the daemon back to its
 * branch-derived local lookup, which is what every spawn used before the id existed.
 *
 * All three session types carry it. The daemon threads `stack_node_id` to the cursor-cli and tool
 * spawn paths as well, so omitting it there would silently re-open the same hole for those.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D34, D36).
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-stack-node-id";
const HOST_ID = "local";

/** The orchestrator whose plan holds the node — the parent the PR-Stack row pre-filled. */
const ORCHESTRATOR = "pr-stack-session-8100-0000-0000-0000-000000000081";
/** A second orchestrator, whose plan holds no node by that id. */
const ANOTHER_ORCHESTRATOR = "pr-stack-session-8200-0000-0000-0000-000000000082";

/** The planned node the row was started from. */
const NODE_ID = "n2";
const PLANNED_BRANCH = "feature/attach-docs/attach-store";

function anOrchestratorSession(sessionId: string) {
  return {
    sessionId,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    orchestratorSessionId: "",
    branch: "",
    daemonInstanceId: HOST_ID,
  };
}

/**
 * A backend that lists both orchestrators (so the parent picker offers a re-parent), stubs the
 * catalogs the form loads on mount, and captures the StartSession it submits.
 */
function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [
        {
          projectId: PROJECT_ID,
          name: "stack-node-id-project",
          gitUrl: "https://example.com/stack-node-id.git",
          mainRepoPath: "/home/dev/stack-node-id-project",
          mainBranchRef: "origin/master",
          daemonInstanceId: HOST_ID,
        },
      ],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", name: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", version: "0.1.0" }],
    }))
    .onUnary(ConnectionService.method.listSessions, () => ({
      sessions: [anOrchestratorSession(ORCHESTRATOR), anOrchestratorSession(ANOTHER_ORCHESTRATOR)],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: [],
      defaultRemote: "origin",
    }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: "child-session-stack-node-id-1",
      livekitRoom: "room-child-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }));
}

/** The form as the PR-Stack row opens it: stacked on `ORCHESTRATOR`, materializing `NODE_ID`. */
function mountPaneForPlannedNode(backend: ReturnType<typeof aCreateSessionBackend>) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    withSelectedDaemon(
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
        initialValues={{
          sessionType: "claude-cli",
          projectId: PROJECT_ID,
          stackParent: ORCHESTRATOR,
          stackNodeId: NODE_ID,
          newBranchName: PLANNED_BRANCH,
        }}
      />,
    ),
  );
}

function theSubmittedStart(
  backend: ReturnType<typeof aCreateSessionBackend>,
  assertion: (call: { stackParent: string; stackNodeId: string }) => void,
) {
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    assertion(calls[0] as { stackParent: string; stackNodeId: string });
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
// The node id travels with the spawn
// ---------------------------------------------------------------------------

it("sends the planned node id beside the orchestrator the form was opened for", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPaneForPlannedNode(backend);

  // When
  createSessionPage.submit();

  // Then
  theSubmittedStart(backend, (call) => {
    expect(call.stackParent).to.equal(ORCHESTRATOR);
    expect(call.stackNodeId).to.equal(NODE_ID);
  });
});

it("sends the planned node id for a cursor-cli child", () => {
  // Given — the daemon threads the id to `cursor_cli_spawn` too
  const backend = aCreateSessionBackend();
  mountPaneForPlannedNode(backend);

  // When
  createSessionPage.switchToCursorCliSession();
  createSessionPage.submit();

  // Then
  theSubmittedStart(backend, (call) => {
    expect(call.stackParent).to.equal(ORCHESTRATOR);
    expect(call.stackNodeId).to.equal(NODE_ID);
  });
});

it("sends the planned node id for a tool child", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPaneForPlannedNode(backend);

  // When
  createSessionPage.switchToToolSession();
  createSessionPage.selectAgent("claude@local");
  createSessionPage.submit();

  // Then
  theSubmittedStart(backend, (call) => {
    expect(call.stackParent).to.equal(ORCHESTRATOR);
    expect(call.stackNodeId).to.equal(NODE_ID);
  });
});

// ---------------------------------------------------------------------------
// A node id belongs to exactly one plan
// ---------------------------------------------------------------------------

it("drops the planned node id when the operator stacks the child under a different orchestrator", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPaneForPlannedNode(backend);

  // When the operator re-parents the spawn
  createSessionPage.selectStackParent(ANOTHER_ORCHESTRATOR);
  createSessionPage.submit();

  // Then — the new parent's plan holds no node by that id, and `LinkStackNode` would refuse it with
  // nothing said to the operator (D36); an empty id falls the daemon back to its local branch lookup
  theSubmittedStart(backend, (call) => {
    expect(call.stackParent).to.equal(ANOTHER_ORCHESTRATOR);
    expect(call.stackNodeId).to.equal("");
  });
});

it("drops the planned node id when the operator makes the child standalone", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPaneForPlannedNode(backend);

  // When the operator detaches the spawn from the stack entirely
  createSessionPage.selectStackParent("");
  createSessionPage.submit();

  // Then — a node id with no stack to name it in is a node id nothing can resolve
  theSubmittedStart(backend, (call) => {
    expect(call.stackParent).to.equal("");
    expect(call.stackNodeId).to.equal("");
  });
});

it("sends the planned node id again once the operator re-selects the original orchestrator", () => {
  // Given — the id is derived from the two values, never cleared as a side effect of touching them
  const backend = aCreateSessionBackend();
  mountPaneForPlannedNode(backend);

  // When
  createSessionPage.selectStackParent(ANOTHER_ORCHESTRATOR);
  createSessionPage.selectStackParent(ORCHESTRATOR);
  createSessionPage.submit();

  // Then
  theSubmittedStart(backend, (call) => {
    expect(call.stackParent).to.equal(ORCHESTRATOR);
    expect(call.stackNodeId).to.equal(NODE_ID);
  });
});

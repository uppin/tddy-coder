/**
 * Acceptance tests: a planned PR keeps track of its child session when that session runs on a
 * **different host** than its pr-stack orchestrator.
 *
 * Three of `QueryBranch`'s six legs are answered from the queried daemon's own disk — `session` is a
 * `read_dir` over its sessions directory, `worktree` is `git worktree list` in its checkout, `remote`
 * is `git rev-parse` against its remote-tracking refs — and none of them carries an `unavailable`
 * discriminator. For a child running one host over they all report `exists = false`, which the view
 * used to read as "this never happened": the row went back to offering **Start session** for work
 * already in flight.
 *
 * Presence is the signal that survives the host boundary. A session's coder participant joins the
 * common room as `daemon-<instance>-<sessionId>` and now publishes its stack association —
 * `orchestrator_session_id`, `stack_node_id`, `branch` — in its `session` metadata block, which the
 * drawer already hydrates onto the synthesized cross-host row. The PR-Stack view joins on that.
 *
 * The orphan rule is narrowed rather than dropped: a live participant claiming the node positively
 * proves the session exists, and absence of one still falls through to the `QueryBranch` verdict.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37–D40).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import {
  aFakeCommonRoomWithMetadata,
  withSelectedDaemonRoom,
} from "../support/rpc/withSelectedDaemon";
import type { DaemonHost } from "../../src/lib/participantRole";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import {
  aPlannedNode,
  aStackChildParticipant,
  aStackPlanJson,
  aBranchResolutionResponse,
  type BranchResolutionFixture,
  type StackNodeFixture,
} from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** Host A — the selected host, where the pr-stack orchestrator lives. */
const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
/** Host B — a peer daemon, never selected. The child session runs here. */
const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-9100-0000-0000-0000-000000000091";
const PROJECT_ID = "proj-pr-stack";

/** The child session on host B. A UUID, because the coder identity encodes it as one. */
const REMOTE_CHILD_SESSION_ID = "dddddddd-0000-4000-8000-000000000004";
const NODE_BRANCH = "feature/attach-docs/attach-store";

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: "origin/master",
  daemonInstanceId: HOST_A.instanceId,
};

/** The live cross-host child, as the common room presents it. */
const REMOTE_CHILD_PARTICIPANT = aStackChildParticipant({
  sessionId: REMOTE_CHILD_SESSION_ID,
  daemonInstanceId: HOST_B.instanceId,
  orchestratorSessionId: ORCHESTRATOR_SESSION_ID,
  stackNodeId: "n1",
  branch: NODE_BRANCH,
});

/**
 * A node the orchestrator's host knows nothing about: the child spawned on host B, so the link was
 * never written here and the node records neither a branch nor a session.
 */
const AN_UNLINKED_NODE: StackNodeFixture = aPlannedNode({
  nodeId: "n1",
  title: "Session attachment storage",
  branchSuggestion: NODE_BRANCH,
});

/** The same node after the link landed — it records its child and its branch. */
const A_LINKED_NODE: StackNodeFixture = aPlannedNode({
  nodeId: "n1",
  title: "Session attachment storage",
  branch: NODE_BRANCH,
  sessionId: REMOTE_CHILD_SESSION_ID,
  childState: "Implementing",
});

/** What host A can say about the branch: nothing, because it owns neither the session nor a worktree. */
const INVISIBLE_FROM_HOST_A: BranchResolutionFixture = {
  branch: NODE_BRANCH,
  session: { exists: false },
  worktree: { exists: false },
  remote: { exists: false },
};

function anOrchestratorSession(nodes: StackNodeFixture[]): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-30T09:00:00Z",
    // Active, so the drawer never partitions it into the collapsed "Remaining" section — these
    // specs are about the planned-PR row, not about which drawer section a row lands in.
    status: "active",
    repoPath: "/home/dev/pr-stack-project",
    isActive: true,
    projectId: PROJECT_ID,
    daemonInstanceId: HOST_A.instanceId,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, nodes),
  };
}

/**
 * Open the PR-Stack screen on host A, with `participants` live in the common room. Host A's
 * `ListSessions` returns the orchestrator only — a session on host B is never in it.
 */
function openPrStackScreen(
  nodes: StackNodeFixture[],
  participants: ReadonlyArray<{ identity: string; metadata: string }>,
  resolutionByBranch: Record<string, BranchResolutionFixture>,
) {
  const backend = aSessionsDrawerBackend([anOrchestratorSession(nodes)])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(resolutionByBranch[req.branch] ?? { branch: req.branch }),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));

  mountWithRecordingLiveKitRpc(
    withSelectedDaemonRoom(
      <SessionsDrawerScreen />,
      [HOST_A, HOST_B],
      aFakeCommonRoomWithMetadata(participants),
    ),
    backend,
  );
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
// A live child on another host
// ---------------------------------------------------------------------------

it("marks a planned PR in progress when its child session is live on another host", () => {
  // Given — the node records nothing (the link was written on host B) and host A sees no session
  // When
  openPrStackScreen([AN_UNLINKED_NODE], [REMOTE_CHILD_PARTICIPANT], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });

  // Then — the participant's own claim on the node is what the row reads
  prStackScreenPage.inProgressBadge("n1").should("have.text", "in progress");
});

it("renders the branch a cross-host child created on the node that has none recorded", () => {
  // Given
  // When
  openPrStackScreen([AN_UNLINKED_NODE], [REMOTE_CHILD_PARTICIPANT], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });
  prStackScreenPage.expandRow("n1");

  // Then — the branch exists, on a host this daemon cannot see; the row states it as an owned
  // branch rather than as the planned name it no longer is
  prStackScreenPage.branchName("n1").should("contain.text", NODE_BRANCH);
});

it("opens the cross-host child session from the planned PR's status chip", () => {
  // Given — the node is linked, and the only route to that session is its live participant
  // When
  openPrStackScreen([A_LINKED_NODE], [REMOTE_CHILD_PARTICIPANT], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });
  prStackScreenPage.openBoundSession("n1");

  // Then
  sessionsDrawerPage.expectSessionSelected(REMOTE_CHILD_SESSION_ID);
});

// ---------------------------------------------------------------------------
// The orphan verdict, narrowed
// ---------------------------------------------------------------------------

it("keeps the status chip rather than offering Start session while a participant claims the node", () => {
  // Given — host A reports `session.exists = false` for a session that is very much alive on host B
  // When
  openPrStackScreen([A_LINKED_NODE], [REMOTE_CHILD_PARTICIPANT], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });

  // Then — offering a second spawn for a session mid-turn is the failure this narrows away
  prStackScreenPage.statusChip("n1").should("have.text", "Implementing");
  prStackScreenPage.startSessionBtn("n1").should("not.exist");
});

it("still offers Start session for a linked node no participant claims", () => {
  // Given — the same resolution, with nobody in the room: the recorded child really is gone
  // When
  openPrStackScreen([A_LINKED_NODE], [], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });

  // Then — presence narrows the orphan rule, it does not remove it
  prStackScreenPage.startSessionBtn("n1").should("exist");
  prStackScreenPage.statusChip("n1").should("not.exist");
});

it("offers Start session when the node's recorded child is gone and another session took its branch", () => {
  // Given — the D7 orphan: the node records a child host A can no longer find, and a *different*
  // session in the same stack has since picked up its branch, claiming no node of its own
  const whoeverOwnsTheBranchNow = aStackChildParticipant({
    sessionId: "ffffffff-0000-4000-8000-000000000006",
    daemonInstanceId: HOST_B.instanceId,
    orchestratorSessionId: ORCHESTRATOR_SESSION_ID,
    stackNodeId: "",
    branch: NODE_BRANCH,
  });

  // When
  openPrStackScreen([A_LINKED_NODE], [whoeverOwnsTheBranchNow], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });

  // Then — owning the branch proves some session exists, never that *this node's* child does, so the
  // recovery CTA D7 exists for stays on the row
  prStackScreenPage.startSessionBtn("n1").should("exist");
  prStackScreenPage.statusChip("n1").should("not.exist");
});

it("ignores a live participant working another orchestrator's node of the same id", () => {
  // Given — a node id is unique within one plan, never across plans, so the orchestrator has to
  // match too or every stack would claim every other stack's children
  const anotherOrchestratorsChild = aStackChildParticipant({
    sessionId: "eeeeeeee-0000-4000-8000-000000000005",
    daemonInstanceId: HOST_B.instanceId,
    orchestratorSessionId: "pr-stack-session-0000-0000-0000-0000-000000000000",
    stackNodeId: "n1",
    branch: "feature/some-other-stack/n1",
  });

  // When
  openPrStackScreen([AN_UNLINKED_NODE], [anotherOrchestratorsChild], {
    [NODE_BRANCH]: INVISIBLE_FROM_HOST_A,
  });

  // Then
  prStackScreenPage.inProgressBadge("n1").should("not.exist");
  prStackScreenPage.startSessionBtn("n1").should("exist");
});

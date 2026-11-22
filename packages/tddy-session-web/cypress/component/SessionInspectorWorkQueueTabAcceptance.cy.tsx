/**
 * Acceptance (`#workqueue` 5/14): a session that owns a work queue shows a Work queue tab in its
 * inspector and opens on it; a session without one shows no such tab. No rule here reads a role —
 * the fixtures below are plain sessions.
 *
 * Test ids are the final ones (`#workqueue` 3/14's map: `work-item-…`, `work-queue-…`). The queue RPC is
 * stubbed under today's generated name; `/green` follows `#workqueue` 3/14's rename when it rebases.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-session-work-queue.md
 */

import React from "react";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { appLocationPage } from "../support/pages/appLocationPage";

function aSession(sessionId: string, ownsWorkQueue: boolean) {
  return {
    sessionId,
    createdAt: "2026-11-22T09:00:00Z",
    status: "idle",
    repoPath: `/home/dev/${sessionId}`,
    pid: 0,
    isActive: false,
    projectId: "proj-queue-tab",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "",
    sessionType: "claude-cli",
    sessionRole: "",
    ownsWorkQueue,
    prNumber: 7n,
    prState: "open",
  };
}

const QUEUE_OWNER = aSession("queue-owner-0000-0000-0000-000000000001", true);
const PLAIN_SESSION = aSession("plain-session-0000-0000-0000-000000000002", false);

function aBackendServing(sessions: ReturnType<typeof aSession>[]) {
  const added: string[] = [];
  const backend = aSessionsDrawerBackend(sessions)
    .onUnary(PrStackService.method.getProjectStack, () => ({
      stackPlanJson: JSON.stringify({ version: 1, nodes: [] }),
    }))
    .onUnary(PrStackService.method.addPlannedPr, (req) => {
      added.push(req.title);
      return {
        stackPlanJson: JSON.stringify({
          version: 1,
          nodes: [{ node_id: "n1", title: req.title, description: req.description, parents: [] }],
        }),
      };
    });
  return { backend, added };
}

function openInspectorOf(sessionId: string, sessions: ReturnType<typeof aSession>[]) {
  const served = aBackendServing(sessions);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), served.backend);
  sessionsDrawerPage.drawerItem(sessionId).click();
  sessionsDrawerPage.inspectorToggle().click();
  return served;
}

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  appLocationPage.reset();
});

it("a session owning a work queue shows the Work queue tab and opens on it", () => {
  // When — the session also has a PR; the queue still comes first
  openInspectorOf(QUEUE_OWNER.sessionId, [QUEUE_OWNER]);

  // Then
  byTestId("sessions-inspector-tab-work-queue").should("have.attr", "aria-selected", "true");
  byTestId("work-queue-view").should("be.visible");
});

it("a session without a queue shows no Work queue tab and opens on PR or Worktree", () => {
  // When
  openInspectorOf(PLAIN_SESSION.sessionId, [PLAIN_SESSION]);

  // Then
  byTestId("sessions-inspector-tab-pr").should("have.attr", "aria-selected", "true");
  byTestId("sessions-inspector-tab-work-queue").should("not.exist");
});

it("the user adds a work item from the Work queue tab", () => {
  // Given
  const served = openInspectorOf(QUEUE_OWNER.sessionId, [QUEUE_OWNER]);

  // When
  byTestId("work-item-add-btn").click();
  byTestId("work-item-add-title-input").type("Wire the inbox");
  byTestId("work-item-add-description-input").type("Read PR status through it");
  byTestId("work-item-add-submit-btn").click();

  // Then
  cy.wrap(served).its("added").should("deep.equal", ["Wire the inbox"]);
});

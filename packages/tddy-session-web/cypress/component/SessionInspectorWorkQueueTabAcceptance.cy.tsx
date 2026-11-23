/**
 * Acceptance (`#workqueue` 5/14, sidebar UX): a session that owns a work queue shows it in a right
 * sidebar (not an inspector tab) and opens on it; a session without one shows neither the sidebar
 * nor its toggle. No rule here reads a role — the fixtures below are plain sessions.
 *
 * Test ids and the queue RPC (`WorkQueueService`) are `#workqueue` 3/14's names.
 *
 * Feature doc: docs/ft/coder/pr-stacking.md § Where a stack lives
 */

import React from "react";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
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
    .onUnary(WorkQueueService.method.getWorkQueue, () => ({
      workQueueJson: JSON.stringify({ version: 1, nodes: [] }),
    }))
    .onUnary(WorkQueueService.method.addWorkItem, (req) => {
      added.push(req.title);
      return {
        workQueueJson: JSON.stringify({
          version: 1,
          nodes: [{ node_id: "n1", title: req.title, description: req.description, parents: [] }],
        }),
      };
    });
  return { backend, added };
}

function openSessionOf(sessionId: string, sessions: ReturnType<typeof aSession>[]) {
  const served = aBackendServing(sessions);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), served.backend);
  sessionsDrawerPage.drawerItem(sessionId).click();
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

it("a session owning a work queue opens on the work queue sidebar, and the inspector has no Work queue tab", () => {
  // When
  openSessionOf(QUEUE_OWNER.sessionId, [QUEUE_OWNER]);

  // Then
  byTestId("work-queue-sidebar").should("be.visible");
  byTestId("work-queue-view").should("be.visible");
  sessionsDrawerPage.inspectorToggle().click();
  byTestId("sessions-inspector-tab-pr").should("have.attr", "aria-selected", "true");
  byTestId("sessions-inspector-tab-work-queue").should("not.exist");
});

it("a session without a queue shows no work queue sidebar or toggle", () => {
  // When
  openSessionOf(PLAIN_SESSION.sessionId, [PLAIN_SESSION]);

  // Then
  byTestId("sessions-inspector-toggle").should("be.visible");
  byTestId("work-queue-sidebar").should("not.exist");
  byTestId("work-queue-button").should("not.exist");
});

it("the user adds a work item from the work queue sidebar", () => {
  // Given
  const served = openSessionOf(QUEUE_OWNER.sessionId, [QUEUE_OWNER]);

  // When
  byTestId("work-item-add-btn").click();
  byTestId("work-item-add-title-input").type("Wire the inbox");
  byTestId("work-item-add-description-input").type("Read PR status through it");
  byTestId("work-item-add-submit-btn").click();

  // Then
  cy.wrap(served).its("added").should("deep.equal", ["Wire the inbox"]);
});

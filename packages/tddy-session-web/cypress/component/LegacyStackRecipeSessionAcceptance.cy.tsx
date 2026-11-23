/**
 * Acceptance (`#workqueue` 1/14): the `pr-stack` workflow recipe and its screen are removed. A session
 * still recorded with that recipe (or a legacy alias) lists and opens like any other tool workflow
 * session — in the full-screen Workflow Chat Screen. Resuming it is refused by the daemon (tested in
 * tddy-agent-launch); viewing it is not.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-remove-pr-stack-recipe.md
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { workflowChatScreenPage } from "../support/pages/workflowChatScreenPage";

function aToolSessionRecordedWithRecipe(sessionId: string, recipe: string) {
  return {
    sessionId,
    createdAt: "2026-07-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/legacy-stack-project",
    pid: 0,
    isActive: false,
    projectId: "proj-legacy-stack",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe,
    sessionType: "tool",
  };
}

const LEGACY_STACK_SESSION = aToolSessionRecordedWithRecipe(
  "legacy-stack-0000-0000-0000-000000000001",
  "pr-stack",
);

function aLegacySessionsBackend() {
  return aSessionsDrawerBackend([LEGACY_STACK_SESSION]).onUnary(CatalogService.method.listAgents, () =>
    withDeclaredSessionTypes({ agents: [{ id: "claude", label: "Claude", ...servedAgentWire("claude") }] }),
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("a session recorded with the removed stack recipe opens in the workflow chat view", () => {
  // Given
  const backend = aLegacySessionsBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(LEGACY_STACK_SESSION.sessionId).click();

  // Then
  workflowChatScreenPage.screen().should("exist");
  workflowChatScreenPage.chat().should("exist");
});

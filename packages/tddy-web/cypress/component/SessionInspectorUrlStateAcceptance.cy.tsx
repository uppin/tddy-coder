/**
 * Acceptance tests: the session inspector's open/expanded state and active tab, and the worktree
 * Code split pane, round-trip through the URL — so a copied address bar reproduces the pane layout
 * the operator is looking at.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 * Changeset: docs/dev/1-WIP/2026-08-01-web-url-state-routing.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  ConnectionService,
  ListWorktreeDirectoryResponseSchema,
  ReadWorktreeFileResponseSchema,
  WorktreeDirEntrySchema,
} from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import { appLocationPage } from "../support/pages/appLocationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKTREE_PATH = "/home/dev/url-state-project";

const SESSION = {
  sessionId: "inspector-url-0000-0000-0000-000000000001",
  createdAt: "2026-08-01T09:00:00Z",
  status: "idle",
  repoPath: WORKTREE_PATH,
  pid: 0,
  isActive: false,
  projectId: "proj-url-state",
  daemonInstanceId: "",
  workflowGoal: "Inspector URL state",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "",
  sessionType: "claude-cli",
};

/** A backend that also serves the worktree RPCs the Code pane needs. */
function aBackendWithWorktree() {
  const entries = [
    { name: "README.md", isDir: false },
    { name: "src", isDir: true },
  ];
  return aSessionsDrawerBackend([SESSION])
    .onUnary(ConnectionService.method.listWorktreeDirectory, (req) =>
      create(ListWorktreeDirectoryResponseSchema, {
        entries: req.relPath === "" ? entries.map((e) => create(WorktreeDirEntrySchema, e)) : [],
      }),
    )
    .onUnary(ConnectionService.method.readWorktreeFile, () =>
      create(ReadWorktreeFileResponseSchema, {
        contentUtf8: "# URL state\n",
        truncated: false,
        byteSize: BigInt(13),
      }),
    );
}

function mountSessionsDrawer() {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), aBackendWithWorktree());
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: the session list defaults open so drawer rows are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  appLocationPage.reset();
});

// ---------------------------------------------------------------------------
// Inspector tab
// ---------------------------------------------------------------------------

it("choosing an inspector tab records that tab in the URL", () => {
  // Given — a selected session whose inspector the operator opens, landing on the Details tab
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  sessionsDrawerPage.inspectorToggle().click();
  appLocationPage.expectParam("inspector", "details");

  // When
  sessionsDrawerPage.inspectorWorktreeTab().click();

  // Then
  appLocationPage.expectParam("inspector", "worktree");
});

it("an ?inspector= deep link opens the inspector on that tab", () => {
  // Given
  appLocationPage.startAt(`/sessions/${SESSION.sessionId}?inspector=tools`);

  // When
  mountSessionsDrawer();

  // Then
  sessionsDrawerPage.expectInspectorState("open");
  sessionsDrawerPage.inspectorToolsTab().should("have.attr", "aria-selected", "true");
});

it("an unknown ?inspector= value falls back to the Details tab", () => {
  // Given — a tab name that is not one of the inspector's tabs
  appLocationPage.startAt(`/sessions/${SESSION.sessionId}?inspector=not-a-tab`);

  // When
  mountSessionsDrawer();

  // Then — the unknown value is discarded rather than rendering a blank panel
  sessionsDrawerPage.inspectorDetailsTab().should("have.attr", "aria-selected", "true");
  appLocationPage.expectParam("inspector", "details");
});

// ---------------------------------------------------------------------------
// Inspector open / expanded
// ---------------------------------------------------------------------------

it("closing the inspector drops the inspector param from the URL", () => {
  // Given — a selected session whose inspector the operator opened, recorded in the URL
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  sessionsDrawerPage.inspectorToggle().click();
  sessionsDrawerPage.expectInspectorState("open");
  appLocationPage.expectParam("inspector", "details");

  // When
  sessionsDrawerPage.inspectorClose().click();

  // Then
  sessionsDrawerPage.expectInspectorState("closed");
  appLocationPage.expectNoParam("inspector");
});

it("expanding the inspector records full=1 in the URL", () => {
  // Given
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  sessionsDrawerPage.inspectorToggle().click();
  sessionsDrawerPage.expectInspectorState("open");

  // When
  sessionsDrawerPage.inspectorExpand().click();

  // Then
  appLocationPage.expectParam("inspector", "details");
  appLocationPage.expectParam("full", "1");
});

it("an ?inspector=&full=1 deep link opens the inspector expanded", () => {
  // Given
  appLocationPage.startAt(`/sessions/${SESSION.sessionId}?inspector=details&full=1`);

  // When
  mountSessionsDrawer();

  // Then
  sessionsDrawerPage.expectInspectorState("expanded");
});

// ---------------------------------------------------------------------------
// Code split pane
// ---------------------------------------------------------------------------

it("opening the Code pane records code=1 in the URL", () => {
  // Given
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();

  // When
  sessionsDrawerPage.codeToggle().click();

  // Then
  worktreeCodePanePage.pane().should("exist");
  appLocationPage.expectParam("code", "1");
});

it("a ?code=1 deep link opens the Code pane on load", () => {
  // Given
  appLocationPage.startAt(`/sessions/${SESSION.sessionId}?code=1`);

  // When
  mountSessionsDrawer();

  // Then
  worktreeCodePanePage.pane().should("exist");
});

it("closing the Code pane drops the code param from the URL", () => {
  // Given
  appLocationPage.startAt(`/sessions/${SESSION.sessionId}?code=1`);
  mountSessionsDrawer();
  worktreeCodePanePage.pane().should("exist");

  // When
  sessionsDrawerPage.codeToggle().click();

  // Then
  worktreeCodePanePage.pane().should("not.exist");
  appLocationPage.expectNoParam("code");
});

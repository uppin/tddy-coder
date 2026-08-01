/**
 * Acceptance tests: the worktrees screen's project filter round-trips through the URL, so a link to
 * "the worktrees of project X" is shareable.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 * Changeset: docs/dev/1-WIP/2026-08-01-web-url-state-routing.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { WorktreesAppPage } from "../../src/components/worktrees/WorktreesAppPage";
import { WorktreeSizeStatus } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type WorktreeStatsRowInput,
} from "../support/rpc/connectionServiceBackend";
import { ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN } from "../support/rpc/durableSessionBackend";
import { worktreesPage } from "../support/pages/worktreesPage";
import { appLocationPage } from "../support/pages/appLocationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ALPHA_PROJECT = {
  projectId: "proj-alpha",
  name: "alpha",
  gitUrl: "https://example.com/dev/alpha.git",
  mainRepoPath: "/repos/alpha",
  daemonInstanceId: "",
};

const BRAVO_PROJECT = {
  projectId: "proj-bravo",
  name: "bravo",
  gitUrl: "https://example.com/dev/bravo.git",
  mainRepoPath: "/repos/bravo",
  daemonInstanceId: "",
};

const A_ROW: WorktreeStatsRowInput = {
  path: "/repos/alpha/.worktrees/feat-a",
  branchLabel: "feature/a",
  sizeStatus: WorktreeSizeStatus.NONE,
  changedFiles: 0,
  linesAdded: 0n,
  linesRemoved: 0n,
};

function mountWorktrees() {
  const backend = aConnectionServiceBackend({
    projectsOverride: [ALPHA_PROJECT, BRAVO_PROJECT],
    worktreeStatsSnapshot: [A_ROW],
  });
  mountWithRpc(
    withSelectedDaemon(<WorktreesAppPage onNavigate={() => undefined} />),
    backend,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  // `WorktreesAppPage` gates its content on `isAuthenticated`, and the client-side token gate
  // decodes the token's `exp` — a placeholder string fails the gate and the screen never renders.
  // Set it through a queued `cy.window()` so it survives the queued `clearLocalStorage()` above.
  cy.window().then((win) => win.localStorage.setItem(ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN));
  window.location.hash = "/worktrees";
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("choosing a project records it in the URL", () => {
  // Given
  mountWorktrees();
  worktreesPage.projectSelect().should("have.value", ALPHA_PROJECT.projectId);

  // When
  worktreesPage.chooseProject(BRAVO_PROJECT.projectId);

  // Then
  appLocationPage.expectParam("project", BRAVO_PROJECT.projectId);
});

it("a ?project= deep link selects that project on load", () => {
  // Given
  appLocationPage.startAt(`/worktrees?project=${BRAVO_PROJECT.projectId}`);

  // When
  mountWorktrees();

  // Then
  worktreesPage.projectSelect().should("have.value", BRAVO_PROJECT.projectId);
});

it("the resolved project is written back into a URL that carried none", () => {
  // Given — no project param, so the screen falls back to the first listed project
  appLocationPage.startAt("/worktrees");

  // When
  mountWorktrees();

  // Then
  appLocationPage.expectParam("project", ALPHA_PROJECT.projectId);
});

it("a ?project= naming an unregistered project falls back to the first listed project", () => {
  // Given
  appLocationPage.startAt("/worktrees?project=proj-deleted");

  // When
  mountWorktrees();

  // Then
  worktreesPage.projectSelect().should("have.value", ALPHA_PROJECT.projectId);
  appLocationPage.expectParam("project", ALPHA_PROJECT.projectId);
});

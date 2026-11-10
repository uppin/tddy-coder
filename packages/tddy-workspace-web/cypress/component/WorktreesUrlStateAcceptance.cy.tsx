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
import { WorktreesContent } from "../../src/worktrees/WorktreesContent";
import { aNavigationDouble, anAuthDouble } from "tddy-client-web/testkit";
import { WorktreeSizeStatus } from "tddy-client-web/gen/worktree_pb";
import { aWorktreesScreenBackend } from "../support/rpc/worktreesScreenBackend";
import type { WorktreeStatsRowInput } from "../support/rpc/worktreeServiceBackend";
import { worktreesPage } from "../support/pages/worktreesPage";
import { navigationPage } from "../support/pages/navigationPage";

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

/** The navigation double the current test mounts into; `startAt` replaces it before mounting. */
let navigation = aNavigationDouble("/worktrees");
let appLocationPage = navigationPage(navigation);

/** Put the app at `path` (a deep link) before mounting. */
function startAt(path: string) {
  navigation = aNavigationDouble(path);
  appLocationPage = navigationPage(navigation);
}

function mountWorktrees() {
  const { backend } = aWorktreesScreenBackend({
    projectsOverride: [ALPHA_PROJECT, BRAVO_PROJECT],
    worktreeStatsSnapshot: [A_ROW],
  });
  cy.mountWithClients(<WorktreesContent />, backend, {
    navigation,
    auth: anAuthDouble({ sessionToken: "session-token" }),
  });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  startAt("/worktrees");
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
  startAt(`/worktrees?project=${BRAVO_PROJECT.projectId}`);

  // When
  mountWorktrees();

  // Then
  worktreesPage.projectSelect().should("have.value", BRAVO_PROJECT.projectId);
});

it("the resolved project is written back into a URL that carried none", () => {
  // Given — no project param, so the screen falls back to the first listed project
  startAt("/worktrees");

  // When
  mountWorktrees();

  // Then
  appLocationPage.expectParam("project", ALPHA_PROJECT.projectId);
});

it("a ?project= naming an unregistered project falls back to the first listed project", () => {
  // Given
  startAt("/worktrees?project=proj-deleted");

  // When
  mountWorktrees();

  // Then
  worktreesPage.projectSelect().should("have.value", ALPHA_PROJECT.projectId);
  appLocationPage.expectParam("project", ALPHA_PROJECT.projectId);
});

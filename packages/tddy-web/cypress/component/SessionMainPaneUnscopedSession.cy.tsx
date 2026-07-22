/**
 * Component test: the session detail pane must render for an *unscoped* session — one with no
 * `projectId` set — instead of crashing while it resolves the session's project.
 *
 * The worktree Code pane needs a non-empty `project_id`, so `SessionMainPane` resolves one for
 * unscoped sessions from the project registry (longest repo-path prefix). That resolution runs
 * eagerly on every render, so it must tolerate a selected session whose `projectId` is unset —
 * otherwise selecting such a session takes the whole detail pane down.
 *
 * Feature: `docs/ft/web/session-drawer.md` (worktree Code pane / unscoped-session resolution).
 */

import React from "react";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import type { SessionEntry, ProjectEntry } from "../../src/gen/connection_pb";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "unscoped-session-1";
const PROJECT_MAIN_REPO = "/home/dev/acme";

/** An unscoped session: no `projectId` set, checked out in a worktree under a registered project. */
const anUnscopedSession: Partial<SessionEntry> = {
  sessionId: SESSION_ID,
  isActive: true,
  status: "active",
  repoPath: `${PROJECT_MAIN_REPO}/worktrees/feature-x`,
};

/** The project whose main repo is the prefix of the unscoped session's worktree. */
const aRegisteredProject: Partial<ProjectEntry> = {
  projectId: "acme-project",
  mainRepoPath: PROJECT_MAIN_REPO,
};

const aConnectedGrpcAttachment: SessionAttachmentState = {
  status: "connected-grpc",
  sessionId: SESSION_ID,
};

const noopHandlers = {
  inspectorState: "closed" as const,
  onToggleInspector: () => undefined,
  onInspectorClose: () => undefined,
  onInspectorExpand: () => undefined,
  onInspectorRestore: () => undefined,
  onResume: () => undefined,
  onDelete: () => undefined,
  onTerminate: () => undefined,
};

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

/** Mount the detail pane for the unscoped session against a registry that can resolve its project. */
function anUnscopedSessionMainPane(): void {
  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={anUnscopedSession as SessionEntry}
      attachment={aConnectedGrpcAttachment}
      projects={[aRegisteredProject as ProjectEntry]}
      runtimes={[]}
      focusedRuntimeId={null}
    />,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("renders the detail pane for a selected session that has no project id set", () => {
  // Given / When — an unscoped session (no projectId) is selected, resolved against the registry
  anUnscopedSessionMainPane();

  // Then — the detail pane and its header controls render; the project resolution tolerated the
  // missing projectId instead of taking the pane down
  sessionsDrawerPage.detailPane().should("exist");
  sessionsDrawerPage.codeToggle().should("exist");
});

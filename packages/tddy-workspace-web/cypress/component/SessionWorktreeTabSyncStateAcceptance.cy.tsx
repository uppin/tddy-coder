/**
 * Acceptance (`#workqueue` 4/14): the session inspector's Worktree tab shows the worktree's dirty files
 * and its branch's relation to the upstream, and offers Pull (fast-forward) or Push only when they apply.
 *
 * Test ids: `session-worktree-sync-status`, `session-worktree-dirty-file-<path>`,
 * `session-worktree-unpushed-<sha>`, `session-worktree-pull`, `session-worktree-push`.
 *
 * Feature: docs/ft/web/session-worktree-inspector.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  WorktreeCommitLineSchema,
  WorktreeDirtyFileSchema,
  WorktreeService,
  WorktreeStatsEventSchema,
  WorktreeSyncStateSchema,
  WorktreeSyncStatus,
  type WorktreeSyncState,
} from "tddy-client-web/gen/worktree_pb";
import { SessionWorktreeTab } from "../../src/worktrees/SessionWorktreeTab";

const REPO_PATH = "/repos/demo/.worktrees/feat-x";

function aBackendReporting(state: WorktreeSyncState) {
  return anInMemoryRpcBackend().implement(WorktreeService, {
    streamWorktreeStats: async function* () {
      yield create(WorktreeStatsEventSchema, { snapshot: [] });
      await new Promise<never>(() => undefined);
    },
    getWorktreeSyncState: async () => state,
  });
}

function mountTabWith(state: WorktreeSyncState) {
  const backend = aBackendReporting(state);
  cy.mountWithClients(
    <SessionWorktreeTab
      client={createClient(WorktreeService, backend.transport())}
      sessionToken="fake-token"
      projectId="proj-sync"
      sessionId="sess-sync-1"
      repoPath={REPO_PATH}
    />,
    backend,
  );
}

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

beforeEach(() => {
  cy.viewport(1280, 800);
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("lists dirty files and names the sync state", () => {
  // Given
  const state = create(WorktreeSyncStateSchema, {
    branch: "feat/x",
    upstream: "origin/feat/x",
    status: WorktreeSyncStatus.UNPUSHED,
    unpushed: [create(WorktreeCommitLineSchema, { sha: "abc1234", subject: "add the thing" })],
    dirtyFiles: [
      create(WorktreeDirtyFileSchema, { path: "src/a.rs", change: "modified" }),
      create(WorktreeDirtyFileSchema, { path: "notes.md", change: "untracked" }),
    ],
    canPush: true,
  });

  // When
  mountTabWith(state);

  // Then
  byTestId("session-worktree-sync-status").should("contain.text", "Unpushed");
  byTestId("session-worktree-unpushed-abc1234").should("contain.text", "add the thing");
  byTestId("session-worktree-dirty-file-src/a.rs").should("contain.text", "modified");
  byTestId("session-worktree-dirty-file-notes.md").should("contain.text", "untracked");
});

it("offers Pull only when unpulled and Push only when unpushed", () => {
  // Given
  const unpulled = create(WorktreeSyncStateSchema, {
    branch: "feat/x",
    upstream: "origin/feat/x",
    status: WorktreeSyncStatus.UNPULLED,
    unpulled: [create(WorktreeCommitLineSchema, { sha: "def5678", subject: "from elsewhere" })],
    canPull: true,
  });

  // When
  mountTabWith(unpulled);

  // Then
  byTestId("session-worktree-pull").should("be.enabled");
  byTestId("session-worktree-push").should("not.exist");
});

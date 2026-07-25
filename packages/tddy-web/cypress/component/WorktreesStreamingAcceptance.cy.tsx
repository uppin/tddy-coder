/**
 * Cypress component acceptance: Worktrees manager screen — lazy, streamed disk usage.
 *
 * `WorktreesAppPage` subscribes to `ConnectionService.StreamWorktreeStats` (daemon-routed, over the
 * shared common-room LiveKit connection) and renders each worktree's size lifecycle live: a first
 * snapshot frame lists every worktree with its status, then per-worktree "updated" frames flip a
 * worktree from Calculating/None to Cached carrying its byte count. The screen also drives
 * `CalculateWorktreeSize` (per-row) and re-subscribes with `recalculate_all` (project-wide).
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 *
 * The in-memory `ConnectionService` backend stubs the stream; assertions on what the daemon received
 * read from its recording spies, never from wire format.
 */

import React from "react";
import { WorktreesAppPage } from "../../src/components/worktrees/WorktreesAppPage";
import { WorktreeSizeStatus } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
  type ConnectionServiceScenario,
  type WorktreeStatsRowInput,
} from "../support/rpc/connectionServiceBackend";
import { ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN } from "../support/rpc/durableSessionBackend";
import { worktreesPage as page } from "../support/pages/worktreesPage";

// ---------------------------------------------------------------------------
// Fixtures — a never-calculated worktree and an already-cached one.
// ---------------------------------------------------------------------------

const NOW = Date.now();

const NONE_ROW: WorktreeStatsRowInput = {
  path: "/repos/demo/.worktrees/feat-a",
  branchLabel: "feature/a",
  sizeStatus: WorktreeSizeStatus.NONE,
  changedFiles: 0,
  linesAdded: 0n,
  linesRemoved: 0n,
};

const CACHED_ROW: WorktreeStatsRowInput = {
  path: "/repos/demo/.worktrees/feat-c",
  branchLabel: "feature/c",
  sizeStatus: WorktreeSizeStatus.CACHED,
  diskBytes: 1_288_490_189n, // formats as "1.2 GB"
  sizeCalculatedAtUnixMs: BigInt(NOW - 2 * 60_000),
  changedFiles: 7,
  linesAdded: 240n,
  linesRemoved: 18n,
};

/** The `NONE_ROW` worktree after its walk completes: same path, now Cached with a byte count. */
const NONE_ROW_CACHED_UPDATE: WorktreeStatsRowInput = {
  ...NONE_ROW,
  sizeStatus: WorktreeSizeStatus.CACHED,
  diskBytes: 524_288_000n, // formats as "500 MB"
  sizeCalculatedAtUnixMs: BigInt(NOW),
};

function aWorktreesBackend(
  overrides: Partial<ConnectionServiceScenario> = {},
): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    worktreeStatsSnapshot: [NONE_ROW, CACHED_ROW],
    ...overrides,
  });
}

function mountWorktrees(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<WorktreesAppPage onNavigate={() => undefined} />), backend);
}

// ---------------------------------------------------------------------------

describe("Worktrees screen — streamed disk usage", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    // A real, unexpired access token — WorktreesAppPage gates its content on `isAuthenticated`, and
    // the client-side token gate decodes the token's `exp` (a malformed value fails the gate). Set it
    // through a queued `cy.window()` so it survives the queued `clearLocalStorage()` above.
    cy.window().then((win) => win.localStorage.setItem(ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN));
  });

  it("renders each streamed worktree with its size status", () => {
    // Given a daemon streaming a snapshot of a never-calculated and an already-cached worktree
    const backend = aWorktreesBackend();

    // When the Worktrees manager screen is mounted
    mountWorktrees(backend);

    // Then each row shows the status carried by the snapshot
    page.status(0).should("contain.text", "None");
    page.status(1).should("contain.text", "Cached");
  });

  it("shows the formatted size for a worktree that streams a Cached update", () => {
    // Given a daemon that, after the snapshot, streams the first worktree's finished size
    const backend = aWorktreesBackend({ worktreeStatsUpdate: NONE_ROW_CACHED_UPDATE });

    // When the Worktrees manager screen is mounted
    mountWorktrees(backend);

    // Then that worktree flips to Cached and shows its formatted size
    page.status(0).should("contain.text", "Cached");
    page.row(0).should("contain.text", "500 MB");
  });

  it("re-subscribes with recalculate-all when Recalculate all is pressed", () => {
    // Given a mounted screen whose first (lazy) subscription has opened
    const backend = aWorktreesBackend();
    mountWorktrees(backend);
    page.status(0).should("contain.text", "None");

    // When the operator presses Recalculate all
    page.recalculateAll().click();

    // Then a StreamWorktreeStats subscription was opened with recalculate_all set
    cy.wrap(null).should(() => {
      expect(backend.worktreeStatsRecalculateAllFlags).to.include(true);
    });
  });

  it("requests calculation for a single worktree's path from its Calculate button", () => {
    // Given a mounted screen showing the streamed worktrees
    const backend = aWorktreesBackend();
    mountWorktrees(backend);
    page.status(0).should("contain.text", "None");

    // When the first row's Calculate is pressed
    page.calculateBtn(0).click();

    // Then CalculateWorktreeSize was invoked for exactly that worktree's path
    cy.wrap(null).should(() => {
      expect(backend.calculatedWorktreePaths).to.deep.equal([NONE_ROW.path]);
    });
  });
});

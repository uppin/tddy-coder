/**
 * Page object for WorktreesScreen component tests.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const worktreesPage = {
  menuButton: () => byTestId(TEST_IDS.shellMenuWorktrees),
  screen: () => byTestId(TEST_IDS.worktreesScreen),

  /** The project filter `<select>` that scopes the worktree list. */
  projectSelect: () => byTestId(TEST_IDS.worktreesProjectSelect, { timeout: 5000 }),
  /** Pick the project with the given id in the project filter. */
  chooseProject: (projectId: string) => {
    byTestId(TEST_IDS.worktreesProjectSelect, { timeout: 5000 }).select(projectId);
  },

  table: () => byTestId(TEST_IDS.worktreesTable),
  rows: () => byTestId(TEST_IDS.worktreeRow),
  /** First delete button by default; pass an index for others. */
  deleteBtn: (index = 0) => byTestId(TEST_IDS.worktreeDelete).eq(index),
  confirmDeleteBtn: () => byTestId(TEST_IDS.worktreeDeleteConfirm),
  deletedPath: () => byTestId(TEST_IDS.worktreeDeletedPath),

  // --- Lazy, streamed disk usage (docs/ft/web/worktree-disk-usage-streaming.md) ---
  /** The size-status cell for the row at `index`. */
  status: (index = 0) => byTestId(TEST_IDS.worktreeStatus).eq(index),
  /** The "last calculated" label for the row at `index`. */
  lastCalculated: (index = 0) => byTestId(TEST_IDS.worktreeLastCalculated).eq(index),
  /** The row element at `index` (assert size presence/absence via its text). */
  row: (index = 0) => byTestId(TEST_IDS.worktreeRow).eq(index),
  /** The per-row Calculate / Recalculate button for the row at `index`. */
  calculateBtn: (index = 0) => byTestId(TEST_IDS.worktreeCalculate).eq(index),
  /** The screen-level Recalculate-all button. */
  recalculateAll: () => byTestId(TEST_IDS.worktreesRecalculateAll),
  /** Harness span recording the last path passed to `onCalculate`. */
  calculatedPath: () => byTestId(TEST_IDS.worktreesCalculatedPath),
  /** Harness span recording that `onRecalculateAll` fired. */
  recalculatedAll: () => byTestId(TEST_IDS.worktreesRecalculatedAll),
};

/** @deprecated Use `worktreesPage` (lowercase). Kept for backward compatibility. */
export const WorktreesPage = worktreesPage;

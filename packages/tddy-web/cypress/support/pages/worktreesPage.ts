/**
 * Page object for WorktreesScreen component tests.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const worktreesPage = {
  menuButton: () => byTestId(TEST_IDS.shellMenuWorktrees),
  screen: () => byTestId(TEST_IDS.worktreesScreen),
  table: () => byTestId(TEST_IDS.worktreesTable),
  rows: () => byTestId(TEST_IDS.worktreeRow),
  /** First delete button by default; pass an index for others. */
  deleteBtn: (index = 0) => byTestId(TEST_IDS.worktreeDelete).eq(index),
  confirmDeleteBtn: () => byTestId(TEST_IDS.worktreeDeleteConfirm),
  deletedPath: () => byTestId(TEST_IDS.worktreeDeletedPath),
};

/** @deprecated Use `worktreesPage` (lowercase). Kept for backward compatibility. */
export const WorktreesPage = worktreesPage;

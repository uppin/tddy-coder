/**
 * Page object for the Worktrees screen (sidebar of worktrees + the selected worktree's pane).
 */

import { byTestId, TEST_IDS } from "../testIds";

export const worktreesPage = {
  menuButton: () => byTestId(TEST_IDS.shellMenuWorktrees),
  screen: () => byTestId(TEST_IDS.worktreesScreen),

  /** The project `<select>` in the sidebar. */
  projectSelect: () => byTestId(TEST_IDS.worktreesProjectSelect, { timeout: 5000 }),
  /** Pick the project with the given id. */
  chooseProject: (projectId: string) => {
    byTestId(TEST_IDS.worktreesProjectSelect, { timeout: 5000 }).select(projectId);
  },

  rows: () => byTestId(TEST_IDS.worktreeRow),
  /** The sidebar row at `index` (sorted biggest first). */
  row: (index = 0) => byTestId(TEST_IDS.worktreeRow).eq(index),
  rowName: (index = 0) => byTestId(TEST_IDS.worktreeRowName).eq(index),
  rowSize: (index = 0) => byTestId(TEST_IDS.worktreeRowSize).eq(index),
  rowSpinner: (index = 0) => worktreesPage.row(index).find(`[data-testid="${TEST_IDS.worktreeRowSpinner}"]`),
  /** The sidebar's Refresh (recalculate all) button. */
  refresh: () => byTestId(TEST_IDS.worktreesRefresh),
  /** The "Calculating disk usage… n of m" progress block; absent when nothing is calculating. */
  progress: () => byTestId(TEST_IDS.worktreesProgress),
  /** The selected worktree's name in the pane header. */
  selectedName: () => byTestId(TEST_IDS.worktreesSelectedName),
  /** The pane — the Session Inspector's Worktree tab. */
  pane: () => byTestId(TEST_IDS.sessionWorktreeTab),
  paneSize: () => byTestId(TEST_IDS.sessionWorktreeSize),
  paneDelete: () => byTestId(TEST_IDS.sessionWorktreeDelete),
  paneDeleteConfirm: () => byTestId(TEST_IDS.sessionWorktreeDeleteConfirm),
};

/** @deprecated Use `worktreesPage` (lowercase). Kept for backward compatibility. */
export const WorktreesPage = worktreesPage;

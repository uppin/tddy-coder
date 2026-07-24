/**
 * Page object for the Session Inspector → Worktree tab acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/session-worktree-inspector.md
 */

import { byTestId, TEST_IDS } from "../testIds";

export const sessionWorktreeTabPage = {
  /** The tab container. */
  tab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeTab, { timeout: 5000, ...options }),

  /** The disk-size readout. */
  size: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeSize, { timeout: 5000, ...options }),

  /** The branch label. */
  branch: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeBranch, { timeout: 5000, ...options }),

  /** The Refresh button. */
  refresh: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeRefresh, { timeout: 5000, ...options }),

  /** The Clear button (first step). */
  clear: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeClear, { timeout: 5000, ...options }),

  /** The Confirm-clear button (second step). */
  confirmClear: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeClearConfirm, { timeout: 5000, ...options }),

  /** The Delete button (first step). */
  delete: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeDelete, { timeout: 5000, ...options }),

  /** The Confirm-delete button (second step). */
  confirmDelete: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeDeleteConfirm, { timeout: 5000, ...options }),

  /** The "worktree missing" state container. */
  missing: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeMissing, { timeout: 5000, ...options }),

  /** The Restore button (missing state). */
  restore: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionWorktreeRestore, { timeout: 5000, ...options }),
};

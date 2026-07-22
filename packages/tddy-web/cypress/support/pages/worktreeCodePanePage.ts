/**
 * Page object for the Worktree Code pane (docs/ft/web/session-code-pane.md).
 *
 * All raw selectors live here; test bodies call named methods — no raw `cy.get(...)` in specs.
 */

import { byTestId, TEST_IDS, worktreeTreeNode } from "../testIds";

export const worktreeCodePanePage = {
  /** The Code toggle button in the main-pane header (present for every session type). */
  toggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeToggle, { timeout: 5000, ...options }),

  /** The split Code pane container (present only when the pane is open). */
  pane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodePane, { timeout: 5000, ...options }),

  /** The directory tree region inside the Code pane. */
  tree: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeFileTree, { timeout: 5000, ...options }),

  /** A single tree node (file or directory) keyed by its path relative to the worktree root. */
  node: (relPath: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(worktreeTreeNode(relPath), { timeout: 5000, ...options }),

  /** The read-only file preview region inside the Code pane. */
  preview: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeFilePreview, { timeout: 5000, ...options }),
};

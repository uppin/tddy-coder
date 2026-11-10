/**
 * Page object for the branch-conflict prompt — the dialog shown when the daemon refuses a session
 * creation because another session already owns the requested branch.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const branchConflictDialogPage = {
  /** The dialog root. */
  dialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.branchConflictDialog, { timeout: 5000, ...options }),

  /** The line naming the owning session and whether it is active. */
  owner: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.branchConflictOwner, { timeout: 5000, ...options }),

  /** Choose "switch to the owning session". */
  chooseSwitch() {
    byTestId(TEST_IDS.branchConflictSwitchBtn).click();
  },

  /** Choose "add another agent on this branch". */
  chooseAddAgent() {
    byTestId(TEST_IDS.branchConflictAddAgentBtn).click();
  },

  /** The editable branch name, pre-filled with the daemon's suggestion. */
  renameInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.branchConflictRenameInput, { timeout: 5000, ...options }),

  /** Type a branch name over the suggestion and re-submit creation under it. */
  renameTo(branch: string) {
    byTestId(TEST_IDS.branchConflictRenameInput).clear().type(branch);
    byTestId(TEST_IDS.branchConflictRenameBtn).click();
  },

  /** Re-submit creation under the pre-filled suggestion, unedited. */
  acceptSuggestedName() {
    byTestId(TEST_IDS.branchConflictRenameBtn).click();
  },

  /** Dismiss the dialog and go back to the creation form. */
  cancel() {
    byTestId(TEST_IDS.branchConflictCancelBtn).click();
  },
};

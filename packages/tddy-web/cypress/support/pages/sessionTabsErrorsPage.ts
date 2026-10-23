/**
 * Page object for the session inspector tabs' failure displays — the VNC, Screen Sharing, Files,
 * Worktree and Tools tabs, and the terminal tab strip. Each getter is the message an operator reads
 * when a load or an action on that surface failed.
 *
 * The controls that cause those failures are driven through the tabs' own page objects
 * (`sessionsDrawerPage`, `sessionFilesTabPage`, `sessionWorktreeTabPage`,
 * `sessionTerminalTabsPage`); the Tools tab has none, so its few controls live here.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import {
  byTestId,
  sessionUploadDeleteError,
  sessionsScreenSharingRemoveError,
  sessionsVncRemoveError,
  sessionsVncStartError,
  TEST_IDS,
} from "../testIds";

type GetOptions = Parameters<typeof cy.get>[1];

export const sessionTabsErrorsPage = {
  // VNC tab
  vncListError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsVncListError, { timeout: 5000, ...options }),
  vncAddError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsVncAddError, { timeout: 5000, ...options }),
  vncStartError: (targetId: string, options?: GetOptions) =>
    byTestId(sessionsVncStartError(targetId), { timeout: 5000, ...options }),
  vncRemoveError: (targetId: string, options?: GetOptions) =>
    byTestId(sessionsVncRemoveError(targetId), { timeout: 5000, ...options }),

  // Screen Sharing tab
  screenSharingListError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsScreenSharingListError, { timeout: 5000, ...options }),
  screenSharingRemoveError: (targetId: string, options?: GetOptions) =>
    byTestId(sessionsScreenSharingRemoveError(targetId), { timeout: 5000, ...options }),

  // Files tab
  filesError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionFilesError, { timeout: 5000, ...options }),
  uploadDeleteError: (fileName: string, options?: GetOptions) =>
    byTestId(sessionUploadDeleteError(fileName), { timeout: 5000, ...options }),

  // Worktree tab
  worktreeError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionWorktreeError, { timeout: 5000, ...options }),

  // Tools tab
  toolListError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsToolListError, { timeout: 5000, ...options }),
  toolCallLogError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsToolCallLogError, { timeout: 5000, ...options }),
  toolCallLog: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsToolCallLog, { timeout: 5000, ...options }),
  toolInvokeResult: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsToolInvokeResult, { timeout: 5000, ...options }),
  /** Press Invoke with whichever tool is selected. */
  invokeTool() {
    byTestId(TEST_IDS.sessionsToolInvokeButton, { timeout: 5000 }).should("not.be.disabled").click();
  },

  // Terminal tab strip
  terminalError: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsTerminalError, { timeout: 10000, ...options }),
};

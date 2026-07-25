/**
 * Page object for the Session Inspector → Files tab acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/session-files-inspector.md
 */

import {
  byTestId,
  TEST_IDS,
  sessionUploadRow,
  sessionUploadSize,
  sessionUploadInsert,
  sessionUploadCopyPath,
  sessionUploadDelete,
  sessionUploadDeleteConfirm,
} from "../testIds";

export const sessionFilesTabPage = {
  /** The Files tab button in the inspector tab strip. */
  tabButton: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionInspectorTabFiles, { timeout: 5000, ...options }),

  /** The Files tab panel container. */
  panel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionFilesPanel, { timeout: 5000, ...options }),

  /** The empty state shown when the session has no uploads. */
  empty: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionFilesEmpty, { timeout: 5000, ...options }),

  /** One uploaded-file row, keyed by file name. */
  row: (fileName: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionUploadRow(fileName), { timeout: 5000, ...options }),

  /** The size readout inside a file's row. */
  size: (fileName: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionUploadSize(fileName), { timeout: 5000, ...options }),

  /** The Insert-into-terminal button of a file's row. */
  insert: (fileName: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionUploadInsert(fileName), { timeout: 5000, ...options }),

  /** The Copy-host-path button of a file's row. */
  copyPath: (fileName: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionUploadCopyPath(fileName), { timeout: 5000, ...options }),

  /** The Delete button (first step) of a file's row. */
  delete: (fileName: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionUploadDelete(fileName), { timeout: 5000, ...options }),

  /** The Confirm-delete button (second step) of a file's row. */
  confirmDelete: (fileName: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionUploadDeleteConfirm(fileName), { timeout: 5000, ...options }),
};

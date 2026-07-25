/**
 * Page object for the terminal drag-to-upload feature. All raw selectors live
 * here; test bodies call named methods.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import { byTestId, TEST_IDS } from "../testIds";

export const terminalFileUploadPage = {
  /** The drop-zone wrapper around the terminal viewport. */
  dropZoneSelector: `[data-testid='${TEST_IDS.ghosttyTerminal}']`,

  /** The overlay shown while a file is dragged over the terminal. */
  dropOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalDropOverlay, { timeout: 5000, ...options }),

  /** The mobile "Attach" button that opens the native file picker. */
  uploadButton: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalUploadButton, { timeout: 5000, ...options }),

  /** The hidden file input inside the Attach button. */
  uploadFileInput: () =>
    byTestId(TEST_IDS.terminalUploadButton).find("input[type='file']"),

  /** The aggregate upload-progress bar. */
  progressIndicator: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.uploadProgressIndicator, { timeout: 5000, ...options }),

  /** The transient per-file upload error. */
  progressError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.uploadProgressError, { timeout: 5000, ...options }),

  /** The progress indicator scoped to inside the host stats footer. */
  progressIndicatorInFooter: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.hostStatsFooter, { timeout: 5000, ...options }).find(
      `[data-testid='${TEST_IDS.uploadProgressIndicator}']`,
    ),
};

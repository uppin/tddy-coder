/**
 * Page object for the Start-Session attachments field on CreateSessionPane.
 */

import {
  byTestId,
  createSessionAttachmentRemove,
  createSessionAttachmentRow,
  TEST_IDS,
} from "../testIds";

export const createSessionAttachmentsPage = {
  field: () => byTestId(TEST_IDS.createSessionAttachmentsField),
  addButton: () => byTestId(TEST_IDS.createSessionAttachmentsAddBtn),
  fileInput: () => byTestId(TEST_IDS.createSessionAttachmentsInput),
  error: () => byTestId(TEST_IDS.createSessionAttachmentsError),
  row: (index: number) => byTestId(createSessionAttachmentRow(index)),
  removeButton: (index: number) => byTestId(createSessionAttachmentRemove(index)),

  /** Pick one file through the hidden native input (same pattern as mobile terminal upload). */
  pickFile(contents: string, fileName: string, mimeType = "text/plain") {
    this.fileInput().selectFile(
      { contents: Cypress.Buffer.from(contents), fileName, mimeType },
      { force: true },
    );
  },
};

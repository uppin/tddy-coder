/**
 * Helpers for driving the terminal file drop-to-upload feature in Cypress
 * component tests: build `File` objects with known bytes, dispatch a real
 * `drop` gesture carrying them, and reconstruct the bytes a fake RPC backend
 * received so a test can assert the upload was faithful.
 *
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

/** A `File` with deterministic UTF-8 bytes, for asserting the uploaded payload. */
export function aFile(name: string, contents: string, type = "text/plain"): File {
  return new File([contents], name, { type });
}

/** Concatenate the ordered chunk byte-arrays recorded for one file into a single string. */
export function reconstructUtf8(chunks: Uint8Array[]): string {
  const total = chunks.reduce((n, c) => n + c.length, 0);
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.length;
  }
  return new TextDecoder().decode(merged);
}

function aDataTransferOf(files: File[]): DataTransfer {
  const dataTransfer = new DataTransfer();
  for (const file of files) {
    dataTransfer.items.add(file);
  }
  return dataTransfer;
}

/** Dispatch only a `dragover` carrying `files` (for asserting the drag overlay). */
export function dragOverWith(selector: string, files: File[]): void {
  cy.get(selector).trigger("dragover", { dataTransfer: aDataTransferOf(files), force: true });
}

/**
 * Dispatch a `dragover` then `drop` on the element at `selector`, carrying `files`
 * in a real `DataTransfer` (as the browser does for an OS file drag).
 */
export function dropFilesOnto(selector: string, files: File[]): void {
  const dataTransfer = aDataTransferOf(files);
  cy.get(selector).trigger("dragover", { dataTransfer, force: true });
  cy.get(selector).trigger("drop", { dataTransfer, force: true });
}

/**
 * Splits a dropped file into ordered upload chunks. The web drives chunking so upload progress is
 * known client-side and one unary RPC (`UploadSessionFileChunk`) works over both transports.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

/** Chunk size for uploaded file bytes (256 KiB). */
export const UPLOAD_CHUNK_SIZE = 256 * 1024;

/**
 * Slices `file` into ordered `Blob` chunks of at most `size` bytes that reassemble to the original
 * bytes. A zero-byte file yields exactly one empty chunk so the final chunk still fires and the
 * host path is returned.
 */
export function chunkFile(file: File, size: number = UPLOAD_CHUNK_SIZE): Blob[] {
  if (file.size === 0) return [file.slice(0, 0)];
  const chunks: Blob[] = [];
  for (let offset = 0; offset < file.size; offset += size) {
    chunks.push(file.slice(offset, offset + size));
  }
  return chunks;
}

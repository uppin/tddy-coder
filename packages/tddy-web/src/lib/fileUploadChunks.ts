/**
 * Splits a dropped file into ordered upload chunks. The web drives chunking so upload progress is
 * known client-side and one unary RPC (`UploadSessionFileChunk`) works over both transports.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

/**
 * Bytes to leave free in a LiveKit data packet for everything in an upload request that is not file
 * data: the RPC envelope (request id, service/method metadata, sender identity) plus the request's
 * own `session_token`, `session_id`, `upload_id`, and `file_name`. Measured overhead for a real
 * request is ~460 bytes; this is a generous multiple of that.
 */
export const UPLOAD_REQUEST_ENVELOPE_HEADROOM = 8 * 1024;

/**
 * Chunk size for uploaded file bytes (48 KiB) — sized so one chunk's whole `UploadSessionFileChunk`
 * request fits in a single LiveKit data packet (see `UPLOAD_REQUEST_ENVELOPE_HEADROOM` and the
 * transport's `MAX_CHUNK_FRAME_BYTES`). Larger chunks make the transport split each request into
 * chunk frames, and a single dropped frame leaves the daemon's reassembler permanently incomplete —
 * the RPC is then never answered, so the upload stalls silently mid-file.
 */
export const UPLOAD_CHUNK_SIZE = 48 * 1024;

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

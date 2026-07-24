/**
 * Unit tests for splitting a dropped file into ordered upload chunks. The web
 * drives chunking so upload progress is known client-side and one unary RPC
 * works over both transports.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import { describe, it, expect } from "bun:test";
import { chunkFile, UPLOAD_CHUNK_SIZE } from "./fileUploadChunks";

async function concatText(chunks: Blob[]): Promise<string> {
  const parts = await Promise.all(chunks.map((c) => c.text()));
  return parts.join("");
}

describe("UPLOAD_CHUNK_SIZE", () => {
  it("is 256 KiB", () => {
    expect(UPLOAD_CHUNK_SIZE).toBe(256 * 1024);
  });
});

describe("chunkFile", () => {
  it("yields a single chunk for a file smaller than the chunk size", () => {
    const file = new File(["hello"], "note.txt");
    const chunks = chunkFile(file);
    expect(chunks).toHaveLength(1);
  });

  it("splits a file into ordered chunks that reassemble to the original bytes", async () => {
    // Given — 10 bytes split at a 4-byte boundary → 3 chunks (4 + 4 + 2)
    const file = new File(["ABCDEFGHIJ"], "data.bin");

    // When
    const chunks = chunkFile(file, 4);

    // Then
    expect(chunks).toHaveLength(3);
    expect(await concatText(chunks)).toBe("ABCDEFGHIJ");
  });

  it("yields exactly one (empty) chunk for a zero-byte file so the final chunk still fires", async () => {
    const file = new File([], "empty.txt");
    const chunks = chunkFile(file);
    expect(chunks).toHaveLength(1);
    expect(await concatText(chunks)).toBe("");
  });
});

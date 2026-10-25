/**
 * Orchestrates a terminal file drop: chunk each dropped file, upload the chunks in order to the
 * host under one per-drop `upload_id`, advance the shared progress store, and type the uploaded
 * files' escaped host paths into the terminal — emulating a native terminal file-drag. A file that
 * fails mid-upload is skipped (its path is not typed) and surfaced as a transient error; the
 * remaining files still upload.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import { useCallback } from "react";
import { chunkFile } from "../lib/fileUploadChunks";
import { randomUuid } from "../lib/randomId";
import { joinQuotedPaths } from "../lib/shellQuote";
import { useUploadProgressController } from "../rpc/uploadProgress";
import { useDaemonClient } from "../rpc/selectedDaemon";
import { SessionFilesService } from "../gen/session_files_pb";

/** One unary chunk upload; resolves to the file's absolute host path on the final chunk. */
export type UploadChunkFn = (args: {
  uploadId: string;
  fileName: string;
  data: Uint8Array;
  last: boolean;
}) => Promise<string>;

export interface UseSessionFileUploadArgs {
  /** Sends one chunk to the host and returns the host path (populated on the last chunk). */
  uploadChunk: UploadChunkFn;
  /** Types text into the terminal input (no newline is appended). */
  insertInput: (text: string) => void;
}

/** One file to upload, however its bytes are reached. */
export interface UploadSource {
  /** The name the host stores it under. */
  name: string;
  size: number;
  /** How many chunks {@link readChunk} serves; at least one, so an empty file still completes. */
  chunkCount: number;
  readChunk(index: number): Promise<Uint8Array>;
}

/** A browser `File` as an upload. */
export function fileUploadSource(file: File): UploadSource {
  const chunks = chunkFile(file);
  return {
    name: file.name,
    size: file.size,
    chunkCount: chunks.length,
    readChunk: async (index) => new Uint8Array(await chunks[index].arrayBuffer()),
  };
}

export interface SessionFileUpload {
  /** Uploads every file under one drop id, then types the successful files' host paths. */
  uploadFiles: (files: File[]) => Promise<void>;
  /** {@link uploadFiles} for files whose bytes are not a browser `File` — a native drop's. */
  uploadSources: (sources: UploadSource[]) => Promise<void>;
}

export function useSessionFileUpload({
  uploadChunk,
  insertInput,
}: UseSessionFileUploadArgs): SessionFileUpload {
  const progress = useUploadProgressController();

  const uploadSources = useCallback(
    async (sources: UploadSource[]) => {
      if (sources.length === 0) return;

      // Not `crypto.randomUUID()`: that is secure-context only, so it is undefined when tddy-web
      // is served over plain http on a LAN address.
      const uploadId = randomUuid();
      const totalBytes = sources.reduce((sum, source) => sum + source.size, 0);
      progress.startDrop(sources.length, totalBytes);

      // Insertion order follows drop order regardless of per-file completion order.
      const successfulPaths: string[] = [];
      for (const source of sources) {
        try {
          let hostPath = "";
          for (let i = 0; i < source.chunkCount; i += 1) {
            const data = await source.readChunk(i);
            const last = i === source.chunkCount - 1;
            hostPath = await uploadChunk({ uploadId, fileName: source.name, data, last });
            progress.advance(data.length);
          }
          // The daemon returns the host path only on the final chunk; never type an empty path.
          if (hostPath !== "") {
            successfulPaths.push(hostPath);
          }
        } catch {
          progress.failFile(source.name);
        }
      }

      if (successfulPaths.length > 0) {
        insertInput(joinQuotedPaths(successfulPaths));
      }
      progress.finishDrop();
    },
    [uploadChunk, insertInput, progress],
  );

  const uploadFiles = useCallback(
    (files: File[]) => uploadSources(files.map(fileUploadSource)),
    [uploadSources],
  );

  return { uploadFiles, uploadSources };
}

/**
 * Deadline for one chunk's upload RPC. Without a deadline a chunk whose request never reaches the
 * daemon (or whose response is lost) leaves the whole drop pending forever — no error, no progress,
 * no typed path. With one, the file is reported as failed and the remaining files still upload.
 * Generous enough that a slow-but-alive link finishes the chunk.
 */
export const UPLOAD_CHUNK_TIMEOUT_MS = 20_000;

/**
 * Builds an {@link UploadChunkFn} bound to the selected daemon's `SessionFilesService`, targeting a
 * given session. Throws (rather than silently no-op'ing) if no daemon is connected, so a failed
 * upload surfaces instead of being dropped.
 */
export function useDaemonUploadChunk(
  sessionToken: string,
  sessionId: string,
  timeoutMs: number = UPLOAD_CHUNK_TIMEOUT_MS,
): UploadChunkFn {
  const client = useDaemonClient(SessionFilesService);
  return useCallback(
    async ({ uploadId, fileName, data, last }) => {
      if (!client) {
        throw new Error("no daemon connected for file upload");
      }
      const resp = await client.uploadSessionFileChunk(
        {
          sessionToken,
          sessionId,
          uploadId,
          fileName,
          data,
          last,
        },
        { timeoutMs },
      );
      return resp.hostPath;
    },
    [client, sessionToken, sessionId, timeoutMs],
  );
}

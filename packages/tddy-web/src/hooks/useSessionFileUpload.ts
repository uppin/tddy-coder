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
import { joinQuotedPaths } from "../lib/shellQuote";
import { useUploadProgressController } from "../rpc/uploadProgress";
import { useDaemonClient } from "../rpc/selectedDaemon";
import { ConnectionService } from "../gen/connection_pb";

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

export interface SessionFileUpload {
  /** Uploads every file under one drop id, then types the successful files' host paths. */
  uploadFiles: (files: File[]) => Promise<void>;
}

export function useSessionFileUpload({
  uploadChunk,
  insertInput,
}: UseSessionFileUploadArgs): SessionFileUpload {
  const progress = useUploadProgressController();

  const uploadFiles = useCallback(
    async (files: File[]) => {
      if (files.length === 0) return;

      const uploadId = crypto.randomUUID();
      const totalBytes = files.reduce((sum, file) => sum + file.size, 0);
      progress.startDrop(files.length, totalBytes);

      // Insertion order follows drop order regardless of per-file completion order.
      const successfulPaths: string[] = [];
      for (const file of files) {
        try {
          const chunks = chunkFile(file);
          let hostPath = "";
          for (let i = 0; i < chunks.length; i += 1) {
            const data = new Uint8Array(await chunks[i].arrayBuffer());
            const last = i === chunks.length - 1;
            hostPath = await uploadChunk({ uploadId, fileName: file.name, data, last });
            progress.advance(data.length);
          }
          // The daemon returns the host path only on the final chunk; never type an empty path.
          if (hostPath !== "") {
            successfulPaths.push(hostPath);
          }
        } catch {
          progress.failFile(file.name);
        }
      }

      if (successfulPaths.length > 0) {
        insertInput(joinQuotedPaths(successfulPaths));
      }
      progress.finishDrop();
    },
    [uploadChunk, insertInput, progress],
  );

  return { uploadFiles };
}

/**
 * Builds an {@link UploadChunkFn} bound to the selected daemon's `ConnectionService`, targeting a
 * given session. Throws (rather than silently no-op'ing) if no daemon is connected, so a failed
 * upload surfaces instead of being dropped.
 */
export function useDaemonUploadChunk(sessionToken: string, sessionId: string): UploadChunkFn {
  const client = useDaemonClient(ConnectionService);
  return useCallback(
    async ({ uploadId, fileName, data, last }) => {
      if (!client) {
        throw new Error("no daemon connected for file upload");
      }
      const resp = await client.uploadSessionFileChunk({
        sessionToken,
        sessionId,
        uploadId,
        fileName,
        data,
        last,
      });
      return resp.hostPath;
    },
    [client, sessionToken, sessionId],
  );
}

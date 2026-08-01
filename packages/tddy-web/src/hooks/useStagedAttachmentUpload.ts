/**
 * Stages the new-session form's local files on the host the form is connected to, on submit.
 *
 * Same shape as the terminal drop upload (`useSessionFileUpload`): the web drives the chunking so
 * one unary RPC works over both transports and progress is known client-side.
 * `UploadStagedAttachmentChunk` is `UploadSessionFileChunk` with `session_id` replaced by
 * `daemon_instance_id` and `upload_id` by `staging_id`, so `chunkFile` / `UPLOAD_CHUNK_SIZE` and the
 * per-file loop are reused unchanged — including the 48 KiB size, which must stay small enough that
 * one request fits one LiveKit data packet (see `lib/fileUploadChunks.ts`).
 *
 * Every file of one submit is staged under a **single** `staging_id`, so the batch is one directory
 * the session host can consume (and, when the session runs elsewhere, fetch from).
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * Feature: docs/ft/coder/session-attachments.md § Staging area
 */

import { useCallback } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../gen/connection_pb";
import { chunkFile } from "../lib/fileUploadChunks";
import { randomUuid } from "../lib/randomId";
import { UPLOAD_CHUNK_TIMEOUT_MS } from "./useSessionFileUpload";

/** One local file to stage, tagged with the caller's own key so progress can be routed to its row. */
export interface StagedAttachmentFile {
  /** Caller-chosen correlation key, echoed back on every progress report for this file. */
  key: string;
  file: File;
}

/** How much of one file has reached the host. */
export interface StagedAttachmentProgress {
  key: string;
  bytesDone: number;
  bytesTotal: number;
}

export interface StageAttachmentFilesArgs {
  /** Host to stage the bytes on. Empty = the daemon handling the call. */
  daemonInstanceId: string;
  /** Staged sequentially, so the loop applies its own backpressure (as the terminal upload does). */
  files: StagedAttachmentFile[];
  /** Called after every chunk lands, so each row can show how far its file has got. */
  onProgress: (progress: StagedAttachmentProgress) => void;
}

export interface StagedAttachmentUpload {
  /**
   * Uploads every file under one freshly generated `staging_id` and resolves to it. Rejects as soon
   * as any file fails: a session must never reference a staged batch that is missing bytes, so a
   * failed staging is a failed creation.
   */
  stageFiles: (args: StageAttachmentFilesArgs) => Promise<string>;
}

export function useStagedAttachmentUpload(
  client: Client<typeof ConnectionService>,
  sessionToken: string,
  timeoutMs: number = UPLOAD_CHUNK_TIMEOUT_MS,
): StagedAttachmentUpload {
  const stageFiles = useCallback(
    async ({ daemonInstanceId, files, onProgress }: StageAttachmentFilesArgs) => {
      // Not `crypto.randomUUID()`: that is secure-context only, so it is undefined when tddy-web is
      // served over plain http on a LAN address.
      const stagingId = randomUuid();
      // TODO: two files sharing a `File.name` in one batch are staged under the same
      // `(staging_id, file_name)`. Uploads are sequential, so the first one completes and writes its
      // `.staged-complete` marker, and the second is then refused by the daemon with
      // "staged file already exists in this batch" — an error, not truncation or corruption. But it
      // surfaces as an opaque daemon failure late in the submit rather than as a form-level refusal
      // next to the offending row. The duplicate-basename check catches the default case, since a
      // row's basename starts as its file name; it does not catch a batch where the operator renamed
      // one of two same-named files. The fix is to make the staged file name unique per row rather
      // than to add a second refusal.
      for (const { key, file } of files) {
        const chunks = chunkFile(file);
        let bytesDone = 0;
        for (let i = 0; i < chunks.length; i += 1) {
          const data = new Uint8Array(await chunks[i]!.arrayBuffer());
          const last = i === chunks.length - 1;
          const resp = await client.uploadStagedAttachmentChunk(
            {
              sessionToken,
              daemonInstanceId,
              stagingId,
              fileName: file.name,
              data,
              last,
            },
            { timeoutMs },
          );
          bytesDone += data.length;
          onProgress({ key, bytesDone, bytesTotal: file.size });
          // The host returns the entry only once it has marked the batch complete. Without it the
          // staged file would be refused at materialization as incomplete, so fail here — where the
          // failure can still name the file — rather than sending a reference to it.
          if (last && resp.entry === undefined) {
            throw new Error(`the host did not confirm the staged upload of ${file.name}`);
          }
        }
      }
      return stagingId;
    },
    [client, sessionToken, timeoutMs],
  );

  return { stageFiles };
}

/**
 * Upload files into the daemon's pre-session staging area for Start-Session attachments.
 * Mirrors `useSessionFileUpload` but targets `UploadStagedAttachmentChunk`.
 */

import { useCallback } from "react";
import { chunkFile } from "../lib/fileUploadChunks";
import { randomUuid } from "../lib/randomId";
import { useDaemonClient } from "../rpc/selectedDaemon";
import { ConnectionService } from "../gen/connection_pb";
import type { StagedAttachmentEntry } from "../gen/connection_pb";
import { UPLOAD_CHUNK_TIMEOUT_MS } from "./useSessionFileUpload";

export type UploadStagedChunkFn = (args: {
  stagingId: string;
  fileName: string;
  data: Uint8Array;
  last: boolean;
}) => Promise<StagedAttachmentEntry | undefined>;

export interface StagedFile {
  /** Basename shown in the session after start. */
  basename: string;
  /** Original picked file name (staging key). */
  fileName: string;
  stagingId: string;
  daemonInstanceId: string;
  sizeBytes: number;
}

export function useStagedAttachmentUpload(sessionToken: string, daemonInstanceId: string) {
  const client = useDaemonClient(ConnectionService);

  const uploadChunk: UploadStagedChunkFn = useCallback(
    async ({ stagingId, fileName, data, last }) => {
      if (!client) {
        throw new Error("no daemon connected for staged attachment upload");
      }
      const resp = await client.uploadStagedAttachmentChunk(
        {
          sessionToken,
          daemonInstanceId,
          stagingId,
          fileName,
          data,
          last,
        },
        { timeoutMs: UPLOAD_CHUNK_TIMEOUT_MS },
      );
      return resp.entry;
    },
    [client, sessionToken, daemonInstanceId],
  );

  const uploadFiles = useCallback(
    async (files: File[]): Promise<StagedFile[]> => {
      if (files.length === 0) return [];
      const stagingId = randomUuid();
      const staged: StagedFile[] = [];

      for (const file of files) {
        const chunks = chunkFile(file);
        let entry: StagedAttachmentEntry | undefined;
        for (let i = 0; i < chunks.length; i += 1) {
          const data = new Uint8Array(await chunks[i].arrayBuffer());
          const last = i === chunks.length - 1;
          entry = await uploadChunk({
            stagingId,
            fileName: file.name,
            data,
            last,
          });
        }
        if (!entry) {
          throw new Error(`staged upload for ${file.name} did not complete`);
        }
        staged.push({
          basename: file.name,
          fileName: file.name,
          stagingId,
          daemonInstanceId: entry.daemonInstanceId,
          sizeBytes: Number(entry.sizeBytes),
        });
      }

      return staged;
    },
    [uploadChunk],
  );

  return { uploadFiles };
}

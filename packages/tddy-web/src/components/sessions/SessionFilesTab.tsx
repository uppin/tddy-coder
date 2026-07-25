import React, { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionUploadEntry } from "../../gen/connection_pb";
import { Button } from "../ui/button";
import { formatBytes } from "./formatTraffic";
import { copyToClipboard } from "../../lib/clipboard";
import { HOST_PATH_MIME } from "../../lib/hostPathDrag";

export interface SessionFilesTabProps {
  client: Client<typeof ConnectionService> | null;
  sessionToken: string;
  sessionId: string;
  /** Inserts a file's host path into the focused terminal (shell-escaping is the caller's job). */
  onInsertPath: (hostPath: string) => void;
  /** Closes the Inspector so the terminal underneath becomes the drop/insert target. */
  onCloseInspector: () => void;
}

/**
 * Session Inspector → Files tab: lists the files already uploaded to the session
 * (`{session_dir}/uploads/{upload_id}/{file_name}`) and makes them repeatedly usable — drag a row
 * onto the terminal, Insert its host path, Copy its host path, or Delete the upload (two-step
 * confirm). Starting a drag or pressing Insert closes the Inspector so the terminal beneath it
 * becomes the drop target. See docs/ft/web/session-files-inspector.md.
 */
export function SessionFilesTab({
  client,
  sessionToken,
  sessionId,
  onInsertPath,
  onCloseInspector,
}: SessionFilesTabProps) {
  const [uploads, setUploads] = useState<SessionUploadEntry[]>([]);
  // Keyed per row (upload_id + file_name), not per upload_id: one drag gesture can upload several
  // files under the same upload_id, so keying by upload_id alone would arm every sibling's confirm.
  const [pendingDelete, setPendingDelete] = useState<Set<string>>(new Set());

  const load = useCallback(async () => {
    if (!client) return;
    const res = await client.listSessionUploads({ sessionToken, sessionId });
    setUploads(res.uploads);
  }, [client, sessionToken, sessionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const rowKey = (entry: SessionUploadEntry) => entry.uploadId + "/" + entry.fileName;

  async function onConfirmDelete(entry: SessionUploadEntry) {
    if (!client) return;
    await client.deleteSessionUpload({
      sessionToken,
      sessionId,
      uploadId: entry.uploadId,
      fileName: entry.fileName,
    });
    setPendingDelete((prev) => {
      const next = new Set(prev);
      next.delete(rowKey(entry));
      return next;
    });
    await load();
  }

  function onInsert(entry: SessionUploadEntry) {
    onInsertPath(entry.hostPath);
    onCloseInspector();
  }

  function onDragStart(e: React.DragEvent<HTMLDivElement>, entry: SessionUploadEntry) {
    e.dataTransfer.setData(HOST_PATH_MIME, entry.hostPath);
    onCloseInspector();
  }

  return (
    <div
      data-testid="session-files-panel"
      className="px-3 py-3 flex flex-col gap-2 text-xs text-muted-foreground"
    >
      {uploads.length === 0 ? (
        <div data-testid="session-files-empty" className="flex flex-col gap-1">
          <p>No files uploaded to this session yet.</p>
          <p>Drop files on the terminal to upload them.</p>
        </div>
      ) : (
        uploads.map((entry) => {
          const confirming = pendingDelete.has(rowKey(entry));
          return (
            <div
              key={rowKey(entry)}
              data-testid={`session-upload-row-${entry.fileName}`}
              draggable
              onDragStart={(e) => onDragStart(e, entry)}
              className="flex flex-col gap-1 rounded border border-border px-2 py-2 cursor-grab"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-foreground break-all">{entry.fileName}</span>
                <span
                  data-testid={`session-upload-size-${entry.fileName}`}
                  className="text-muted-foreground tabular-nums flex-shrink-0"
                >
                  {formatBytes(Number(entry.sizeBytes))}
                </span>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  data-testid={`session-upload-insert-${entry.fileName}`}
                  onClick={() => onInsert(entry)}
                >
                  Insert
                </Button>
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  data-testid={`session-upload-copy-path-${entry.fileName}`}
                  onClick={() => {
                    void copyToClipboard(entry.hostPath);
                  }}
                >
                  Copy path
                </Button>
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  data-testid={`session-upload-delete-${entry.fileName}`}
                  onClick={() => {
                    setPendingDelete((prev) => new Set(prev).add(rowKey(entry)));
                  }}
                >
                  Delete
                </Button>
                {confirming ? (
                  <Button
                    type="button"
                    size="xs"
                    variant="destructive"
                    data-testid={`session-upload-delete-confirm-${entry.fileName}`}
                    disabled={!client}
                    onClick={() => {
                      void onConfirmDelete(entry);
                    }}
                  >
                    Confirm delete
                  </Button>
                ) : null}
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

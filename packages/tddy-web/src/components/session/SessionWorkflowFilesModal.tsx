import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";

import { SessionFilesPanel, type WorkflowFileRow } from "./SessionFilesPanel";

export type SessionWorkflowFilesModalProps = {
  open: boolean;
  onClose: () => void;
  sessionId: string;
  sessionToken: string | null;
  client: Client<typeof ConnectionService>;
};

/**
 * Lists allowlisted workflow files for a session and loads file content on demand when the user
 * selects a basename (ConnectionService RPCs).
 */
export function SessionWorkflowFilesModal({
  open,
  onClose,
  sessionId,
  sessionToken,
  client,
}: SessionWorkflowFilesModalProps) {
  const [files, setFiles] = useState<WorkflowFileRow[]>([]);
  const [fileContents, setFileContents] = useState<Record<string, string>>({});
  const [selectedBasename, setSelectedBasename] = useState("");
  const [listError, setListError] = useState<string | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const loadedRef = useRef(new Set<string>());

  useEffect(() => {
    if (!open) {
      loadedRef.current = new Set();
      setFiles([]);
      setFileContents({});
      setSelectedBasename("");
      setListError(null);
    }
  }, [open]);

  useEffect(() => {
    if (!open || !sessionToken || !sessionId.trim()) {
      return;
    }
    let cancelled = false;
    setLoadingList(true);
    setListError(null);
    void client
      .listSessionWorkflowFiles({ sessionToken, sessionId: sessionId.trim() })
      .then((res) => {
        if (cancelled) return;
        const rows = res.files.map((f) => ({ basename: f.basename }));
        setFiles(rows);
        const first = rows[0]?.basename ?? "";
        setSelectedBasename(first);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setListError(e instanceof Error ? e.message : "Failed to list workflow files");
          setFiles([]);
          setSelectedBasename("");
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingList(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, sessionToken, sessionId, client]);

  const loadContent = useCallback(
    async (basename: string) => {
      if (!sessionToken || !basename.trim() || !sessionId.trim()) return;
      if (loadedRef.current.has(basename)) return;
      loadedRef.current.add(basename);
      try {
        const res = await client.readSessionWorkflowFile({
          sessionToken,
          sessionId: sessionId.trim(),
          basename,
        });
        setFileContents((prev) => ({ ...prev, [basename]: res.contentUtf8 }));
      } catch (e: unknown) {
        loadedRef.current.delete(basename);
        const msg = e instanceof Error ? e.message : String(e);
        setFileContents((prev) => ({
          ...prev,
          [basename]: `Could not load file: ${msg}`,
        }));
      }
    },
    [client, sessionToken, sessionId],
  );

  useEffect(() => {
    if (!open || !selectedBasename) return;
    void loadContent(selectedBasename);
  }, [open, selectedBasename, loadContent]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="session-workflow-files-title"
        className="flex max-h-[min(90vh,720px)] w-full max-w-4xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-lg"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
          <h2 id="session-workflow-files-title" className="text-lg font-semibold">
            Session workflow files
          </h2>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-muted"
            onClick={onClose}
            data-testid="session-workflow-files-close"
          >
            Close
          </button>
        </header>
        <p className="shrink-0 px-4 pt-2 font-mono text-xs text-muted-foreground">
          Session: {sessionId}
        </p>
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {loadingList ? (
            <p className="text-sm text-muted-foreground">Loading file list…</p>
          ) : listError ? (
            <p className="text-sm text-destructive" data-testid="session-workflow-files-list-error">
              {listError}
            </p>
          ) : files.length === 0 ? (
            <p className="text-sm text-muted-foreground">No allowlisted workflow files in this session.</p>
          ) : (
            <SessionFilesPanel
              files={files}
              fileContents={fileContents}
              selectedBasename={selectedBasename}
              onSelectBasename={setSelectedBasename}
            />
          )}
        </div>
      </div>
    </div>
  );
}

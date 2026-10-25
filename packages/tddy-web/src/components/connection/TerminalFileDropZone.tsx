/**
 * Wraps the terminal viewport so files dragged from the host OS upload to the session dir and
 * their host paths are typed into the terminal. A drag over the region shows an overlay; dropping
 * reads `dataTransfer.files` and runs the upload → type-path flow.
 *
 * Inside Tddy Desktop the window's native drag-drop handler owns OS drags, so the HTML5 handlers
 * below never fire for them; the same drop arrives through `nativeFileDrop`'s provider instead,
 * carrying the files' real paths. A session that reads this machine's files (`readsLocalPaths`)
 * gets those paths typed; any other session gets the files uploaded, read off this machine through
 * the window. A folder has no bytes to upload, so it is refused with a visible message.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import React, { useEffect, useRef, useState } from "react";
import { useSessionFileUpload, useDaemonUploadChunk } from "../../hooks/useSessionFileUpload";
import { joinQuotedPaths } from "../../lib/shellQuote";
import { HOST_PATH_MIME } from "../../lib/hostPathDrag";
import { droppedFileUploadSource, useNativeFileDropTarget } from "./nativeFileDrop";

/** How long a refused drop's message stays up. */
const DROP_REFUSAL_VISIBLE_MS = 8000;

export interface TerminalFileDropZoneProps {
  sessionToken: string;
  sessionId: string;
  /** Types the uploaded files' host paths into the terminal input (no newline appended). */
  insertInput: (text: string) => void;
  /**
   * The session's agent reads this machine's filesystem (`SessionEntry.readsHostPaths`, and the
   * session is on the host application's own daemon). A file dropped from the OS then has its real
   * path typed; otherwise it is uploaded into the session directory. Only the host application's
   * native drop knows real paths, so a browser drop always uploads.
   */
  readsLocalPaths?: boolean;
  children: React.ReactNode;
}

export function TerminalFileDropZone({
  sessionToken,
  sessionId,
  insertInput,
  readsLocalPaths = false,
  children,
}: TerminalFileDropZoneProps) {
  const [dragging, setDragging] = useState(false);
  const [nativeHover, setNativeHover] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const zoneRef = useRef<HTMLDivElement>(null);
  const uploadChunk = useDaemonUploadChunk(sessionToken, sessionId);
  const { uploadFiles, uploadSources } = useSessionFileUpload({
    uploadChunk,
    insertInput,
  });

  const nativeDrop = useNativeFileDropTarget(zoneRef, {
    onHover: setNativeHover,
    onDrop: (paths) => void deliverNativeDrop(paths),
  });

  useEffect(() => {
    if (refusal === null) return;
    const timer = setTimeout(() => setRefusal(null), DROP_REFUSAL_VISIBLE_MS);
    return () => clearTimeout(timer);
  }, [refusal]);

  async function deliverNativeDrop(paths: string[]) {
    if (!nativeDrop || paths.length === 0) return;
    setRefusal(null);
    if (readsLocalPaths) {
      insertInput(joinQuotedPaths(paths));
      return;
    }
    let described;
    try {
      described = await nativeDrop.describe(paths);
    } catch (error) {
      setRefusal(`The dropped files could not be read: ${String(error)}`);
      return;
    }
    const folders = described.filter((entry) => !entry.isFile);
    if (folders.length > 0) {
      setRefusal(
        `Folders can't be sent to this session — drop the files inside them instead: ${folders
          .map((folder) => folder.name)
          .join(", ")}`,
      );
    }
    const files = described.filter((entry) => entry.isFile);
    await uploadSources(files.map((file) => droppedFileUploadSource(nativeDrop, file)));
  }

  const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    if (!dragging) setDragging(true);
  };

  const handleDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
    // Only clear when the pointer leaves the drop zone entirely (not a child boundary crossing).
    if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
    setDragging(false);
  };

  const handleDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setDragging(false);
    // A file dragged out of the Files tab carries its already-uploaded host path — insert the
    // quoted path without re-uploading (the bytes are already on the host).
    const hostPath = e.dataTransfer.getData(HOST_PATH_MIME);
    if (hostPath) {
      insertInput(joinQuotedPaths([hostPath]));
      return;
    }
    const files = Array.from(e.dataTransfer.files);
    if (files.length > 0) {
      void uploadFiles(files);
    }
  };

  return (
    <div
      ref={zoneRef}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      style={{ position: "relative", width: "100%", height: "100%", minWidth: 0, minHeight: 0 }}
    >
      {children}
      {(dragging || nativeHover) && (
        <div
          data-testid="terminal-drop-overlay"
          className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center border-2 border-dashed border-primary bg-background/70 text-sm text-foreground"
        >
          {nativeHover && readsLocalPaths
            ? "Drop to insert the file paths"
            : "Drop files to upload to the session"}
        </div>
      )}
      {refusal !== null && (
        <div
          data-testid="terminal-drop-refused"
          role="alert"
          className="pointer-events-none absolute bottom-2 left-2 z-10 max-w-[90%] rounded border border-destructive/40 bg-background/90 px-2 py-1 text-xs text-destructive"
        >
          {refusal}
        </div>
      )}
    </div>
  );
}

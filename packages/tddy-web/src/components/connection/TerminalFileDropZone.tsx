/**
 * Wraps the terminal viewport so files dragged from the host OS upload to the session dir and
 * their host paths are typed into the terminal. A drag over the region shows an overlay; dropping
 * reads `dataTransfer.files` and runs the upload → type-path flow.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import React, { useState } from "react";
import { useSessionFileUpload, useDaemonUploadChunk } from "../../hooks/useSessionFileUpload";

export interface TerminalFileDropZoneProps {
  sessionToken: string;
  sessionId: string;
  /** Types the uploaded files' host paths into the terminal input (no newline appended). */
  insertInput: (text: string) => void;
  children: React.ReactNode;
}

export function TerminalFileDropZone({
  sessionToken,
  sessionId,
  insertInput,
  children,
}: TerminalFileDropZoneProps) {
  const [dragging, setDragging] = useState(false);
  const uploadChunk = useDaemonUploadChunk(sessionToken, sessionId);
  const { uploadFiles } = useSessionFileUpload({ uploadChunk, insertInput });

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
    const files = Array.from(e.dataTransfer.files);
    if (files.length > 0) {
      void uploadFiles(files);
    }
  };

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      style={{ position: "relative", width: "100%", height: "100%", minWidth: 0, minHeight: 0 }}
    >
      {children}
      {dragging && (
        <div
          data-testid="terminal-drop-overlay"
          className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center border-2 border-dashed border-primary bg-background/70 text-sm text-foreground"
        >
          Drop files to upload to the session
        </div>
      )}
    </div>
  );
}

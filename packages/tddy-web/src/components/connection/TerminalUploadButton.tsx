/**
 * "Attach" button for the mobile Keyboard strip. Mobile has no OS drag-and-drop, so this opens the
 * native multi-file picker; picking files runs the same upload → type-path flow as a desktop drop.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § Mobile UX — File upload from the Keyboard strip
 */

import React from "react";
import { useSessionFileUpload, useDaemonUploadChunk } from "../../hooks/useSessionFileUpload";

export interface TerminalUploadButtonProps {
  sessionToken: string;
  sessionId: string;
  /** Types the uploaded files' host paths into the terminal input (no newline appended). */
  insertInput: (text: string) => void;
  className?: string;
}

const DEFAULT_LABEL_CLASS =
  "relative inline-flex shrink-0 cursor-pointer items-center rounded border border-input bg-background px-3 py-1 text-xs text-foreground";

export function TerminalUploadButton({
  sessionToken,
  sessionId,
  insertInput,
  className,
}: TerminalUploadButtonProps) {
  const uploadChunk = useDaemonUploadChunk(sessionToken, sessionId);
  const { uploadFiles } = useSessionFileUpload({ uploadChunk, insertInput });

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    // Reset so re-picking the same file fires `change` again.
    e.target.value = "";
    if (files.length > 0) {
      void uploadFiles(files);
    }
  };

  return (
    <label data-testid="terminal-upload-button" className={className ?? DEFAULT_LABEL_CLASS}>
      <input
        type="file"
        multiple
        aria-label="Attach files to upload to the session"
        className="absolute inset-0 h-full w-full opacity-0"
        style={{ margin: 0, border: "none", fontSize: 1 }}
        onChange={handleChange}
      />
      <span className="pointer-events-none">Attach</span>
    </label>
  );
}

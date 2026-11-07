/**
 * The new-session form's attachments section: the drop target for an OS file drag, the native
 * multi-file picker, and the entry point to the host-document picker. The rows render inside it, so
 * dropping onto the list is dropping onto the section.
 *
 * The drag handling mirrors `components/connection/TerminalFileDropZone` — in particular the
 * drag-leave that ignores a child boundary crossing, without which the overlay flickers off as the
 * pointer moves over a row. The picker is a `<label>`-wrapped hidden input, as in
 * `components/connection/TerminalUploadButton`, so the button and the native dialog are one element.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * Feature: docs/ft/coder/session-attachments.md
 */

import React, { useState, type ReactNode } from "react";

export interface AttachmentDropZoneProps {
  /** Files chosen through the picker or dropped on the section, in the order they arrived. */
  onFilesPicked: (files: File[]) => void;
  /** Opens the host-document picker, which attaches by reference instead of uploading. */
  onPickHostDocument: () => void;
  /** True while a creation is in flight — the attachment set is being uploaded and is frozen. */
  disabled: boolean;
  /** The attachment rows and any inline refusal, rendered inside the drop target. */
  children: ReactNode;
}

const affordanceClass =
  "relative inline-flex shrink-0 cursor-pointer items-center rounded-md border border-input bg-background px-3 py-1 text-xs text-foreground hover:bg-muted";

export function AttachmentDropZone({
  onFilesPicked,
  onPickHostDocument,
  disabled,
  children,
}: AttachmentDropZoneProps) {
  const [dragging, setDragging] = useState(false);

  const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    if (!dragging) setDragging(true);
  };

  const handleDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
    // Only clear when the pointer leaves the section entirely (not a child boundary crossing).
    if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
    setDragging(false);
  };

  const handleDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setDragging(false);
    const files = Array.from(e.dataTransfer.files);
    if (files.length > 0) {
      onFilesPicked(files);
    }
  };

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    // Reset so re-picking the same file fires `change` again.
    e.target.value = "";
    if (files.length > 0) {
      onFilesPicked(files);
    }
  };

  return (
    <div
      data-testid="create-session-attachments-section"
      className="relative rounded-md border border-dashed border-input p-2 space-y-2"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className="flex items-center gap-2">
        <span className="text-sm text-muted-foreground">Attachments</span>
        <label data-testid="create-session-attachment-pick-btn" className={affordanceClass}>
          <input
            type="file"
            multiple
            aria-label="Attach documents to the new session"
            className="absolute inset-0 h-full w-full opacity-0"
            style={{ margin: 0, border: "none", fontSize: 1 }}
            disabled={disabled}
            onChange={handleChange}
          />
          <span className="pointer-events-none">Attach files</span>
        </label>
        <button
          type="button"
          data-testid="create-session-attachment-pick-host-doc-btn"
          className={affordanceClass}
          disabled={disabled}
          onClick={onPickHostDocument}
        >
          From host…
        </button>
      </div>

      {children}

      {dragging && (
        <div
          data-testid="create-session-attachment-drop-overlay"
          className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-md border-2 border-dashed border-primary bg-background/70 text-sm text-foreground"
        >
          Drop documents to attach them to the session
        </div>
      )}
    </div>
  );
}

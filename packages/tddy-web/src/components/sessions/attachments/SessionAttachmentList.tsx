/**
 * One row per document attached to the new-session form: where it comes from, the name it will be
 * stored under (editable), its size, a remove button, and its progress once Create is pressed.
 *
 * A rename changes only `SessionAttachment.basename` — the source locator, and the stored bytes, are
 * untouched.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * Feature: docs/ft/coder/session-attachments.md
 */

import React from "react";
import { X } from "lucide-react";
import {
  attachmentSourceLabel,
  type AttachmentProgress,
  type AttachmentProgressByBasename,
  type PendingAttachment,
} from "./pendingAttachment";
import { formatAttachmentBytes } from "../../../lib/attachmentBytes";

export interface SessionAttachmentListProps {
  attachments: PendingAttachment[];
  /** Per-attachment progress while a creation is in flight, keyed by basename. */
  progress: AttachmentProgressByBasename;
  onRename: (id: string, basename: string) => void;
  onRemove: (id: string) => void;
  /** True while a creation is in flight — the rows are then what is being uploaded. */
  disabled: boolean;
}

const PHASE_LABELS: Readonly<Record<AttachmentProgress["phase"], string>> = {
  staging: "Uploading",
  materializing: "Materializing",
};

export function SessionAttachmentList({
  attachments,
  progress,
  onRename,
  onRemove,
  disabled,
}: SessionAttachmentListProps) {
  if (attachments.length === 0) return null;

  return (
    <ul className="space-y-1">
      {attachments.map((attachment) => {
        const rowProgress = progress[attachment.basename];
        return (
          <li
            key={attachment.id}
            data-testid={`create-session-attachment-row-${attachment.basename}`}
            data-attachment-basename={attachment.basename}
            className="flex items-center gap-2 rounded-md border border-input bg-background px-2 py-1 text-sm"
          >
            <span className="shrink-0 text-xs text-muted-foreground">
              {attachmentSourceLabel(attachment.source)}
            </span>
            <input
              type="text"
              aria-label={`Name for the attached document ${attachment.basename}`}
              data-testid={`create-session-attachment-name-${attachment.basename}`}
              className="min-w-0 flex-1 rounded border border-input bg-background px-2 py-0.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              value={attachment.basename}
              disabled={disabled}
              onChange={(e) => onRename(attachment.id, e.target.value)}
            />
            <span
              data-testid={`create-session-attachment-size-${attachment.basename}`}
              className="shrink-0 text-xs text-muted-foreground"
            >
              {formatAttachmentBytes(attachment.sizeBytes)}
            </span>
            {rowProgress !== undefined && (
              <span
                data-testid={`create-session-attachment-progress-${attachment.basename}`}
                data-attachment-percent={String(rowProgress.percent)}
                className="shrink-0 text-xs text-muted-foreground"
              >
                {`${PHASE_LABELS[rowProgress.phase]} ${rowProgress.percent}%`}
              </span>
            )}
            <button
              type="button"
              aria-label={`Remove the attached document ${attachment.basename}`}
              data-testid={`create-session-attachment-remove-${attachment.basename}`}
              className="shrink-0 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
              disabled={disabled}
              onClick={() => onRemove(attachment.id)}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </li>
        );
      })}
    </ul>
  );
}

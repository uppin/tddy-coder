/**
 * Aggregate upload-progress bar for the Host Stats Footer. Renders `"{n} files · {pct}%"` while a
 * drop is in flight and a transient error when a file fails; renders nothing when idle with no
 * error.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/host-stats-footer.md § Upload progress
 */

import React from "react";
import { useUploadProgressSnapshot } from "../../rpc/uploadProgress";

export function UploadProgressIndicator() {
  const { active, fileCount, percent, error } = useUploadProgressSnapshot();

  if (!active && error === null) return null;

  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      {active && (
        <span
          data-testid="upload-progress-indicator"
          data-upload-percent={String(percent)}
          data-upload-file-count={String(fileCount)}
          className="inline-flex items-center gap-2"
        >
          <span
            aria-hidden
            className="inline-block h-1.5 w-16 overflow-hidden rounded bg-muted"
          >
            <span
              className="block h-full bg-primary"
              style={{ width: `${percent}%` }}
            />
          </span>
          <span>
            {fileCount} {fileCount === 1 ? "file" : "files"} · {percent}%
          </span>
        </span>
      )}
      {error !== null && (
        <span data-testid="upload-progress-error" className="text-destructive">
          {error}
        </span>
      )}
    </div>
  );
}

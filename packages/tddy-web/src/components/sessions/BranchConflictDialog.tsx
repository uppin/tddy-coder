/**
 * The branch-conflict prompt: shown when the daemon refuses a session creation because another
 * session already owns the requested branch (`StartSessionResponse.branch_conflict`).
 *
 * Nothing was created when this opens — the operator picks how to proceed: switch to the owning
 * session, run a second agent on the same branch, or name a different branch. Cancelling leaves the
 * creation form behind it untouched, so the choice can be made again.
 *
 * "Switch" is always offered: by definition a conflict means an owning session exists.
 *
 * PRD: docs/ft/daemon/session-branch-conflict.md § Operator prompt
 */

import React, { useState } from "react";
import type { BranchConflict } from "../../gen/connection_pb";
import { Button } from "../ui/button";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface BranchConflictDialogProps {
  /** The refusal to resolve: the requested branch, its owning session, and a free name to suggest. */
  conflict: BranchConflict;
  /** Attach to the owning session instead of creating one. */
  onSwitchToOwner: () => void;
  /** Create a second agent on the owned branch, sharing the owning session's worktree. */
  onAddAgent: () => void;
  /** Re-run creation on `branchName` instead. */
  onRename: (branchName: string) => void;
  /** Dismiss without creating anything. */
  onCancel: () => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function BranchConflictDialog({
  conflict,
  onSwitchToOwner,
  onAddAgent,
  onRename,
  onCancel,
}: BranchConflictDialogProps) {
  const [branchName, setBranchName] = useState(conflict.suggestedBranchName);

  // `owner` is only optional because proto3 message fields always are in the generated types — a
  // reported conflict always names the session that holds the branch.
  const ownerSessionId = conflict.owner?.sessionId ?? "";
  const ownerState = conflict.owner?.isActive === true ? "active" : (conflict.owner?.status ?? "");

  return (
    <div
      data-testid="branch-conflict-dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Branch already in use"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
    >
      <div className="bg-card text-card-foreground border-border flex w-full max-w-md flex-col gap-4 rounded-xl border p-4 shadow-lg">
        <h2 className="text-sm font-semibold">Branch already in use</h2>

        <p data-testid="branch-conflict-owner" className="text-muted-foreground text-sm">
          <code className="text-foreground">{conflict.branch}</code> is already used by session{" "}
          <code className="text-foreground">{ownerSessionId}</code> ({ownerState}). Nothing was
          created.
        </p>

        <div className="flex flex-col gap-2">
          <Button
            type="button"
            data-testid="branch-conflict-switch-btn"
            variant="secondary"
            onClick={onSwitchToOwner}
          >
            Switch to the existing session
          </Button>
          <Button
            type="button"
            data-testid="branch-conflict-add-agent-btn"
            variant="outline"
            onClick={onAddAgent}
          >
            Add another agent on this branch
          </Button>
        </div>

        <div>
          <label className="text-muted-foreground mb-1 block text-sm" htmlFor="branch-conflict-rename">
            Use a different branch name
          </label>
          <div className="flex gap-2">
            <input
              id="branch-conflict-rename"
              data-testid="branch-conflict-rename-input"
              type="text"
              className="border-input bg-background focus-visible:ring-ring w-full rounded-md border px-3 py-1.5 text-sm shadow-sm focus-visible:ring-2 focus-visible:outline-none"
              value={branchName}
              onChange={(e) => setBranchName(e.target.value)}
            />
            <Button
              type="button"
              data-testid="branch-conflict-rename-btn"
              disabled={branchName.trim() === ""}
              onClick={() => onRename(branchName)}
            >
              Create
            </Button>
          </div>
        </div>

        <div className="flex justify-end">
          <Button
            type="button"
            data-testid="branch-conflict-cancel-btn"
            variant="ghost"
            onClick={onCancel}
          >
            Cancel
          </Button>
        </div>
      </div>
    </div>
  );
}

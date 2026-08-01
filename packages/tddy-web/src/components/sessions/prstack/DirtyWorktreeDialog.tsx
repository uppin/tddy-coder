import React, { useState } from "react";
import { Button } from "../../ui/button";

/** The pull the operator asked for, held until they say what to do about the outstanding work. */
export interface DirtyWorktreePrompt {
  nodeId: string;
  /** The branch the base is pulled into — also what the pre-filled commit message names. */
  branch: string;
  /** The base the row's control named, carried through unchanged so the pull matches the promise. */
  baseBranch: string;
  /** The strategy the operator originally clicked; confirming keeps it rather than reverting to merge. */
  strategy: "merge" | "rebase";
  /** The tracked paths with uncommitted changes, as the daemon's last resolution reported them. */
  dirtyPaths: string[];
}

export interface DirtyWorktreeDialogProps {
  /** The pending pull, or null when nothing is waiting on the operator. */
  prompt: DirtyWorktreePrompt | null;
  /** Commit and push the outstanding work with this message, then run the pull. */
  onCommitAndPull: (commitMessage: string) => void;
  /** Abandon the pull entirely — the worktree is left exactly as it was. */
  onCancel: () => void;
}

/**
 * The prompt shown when a pull targets a worktree holding uncommitted changes to tracked files.
 *
 * A dirty worktree is a prompt, not a refusal (D31): refusing outright leaves a permanently dead
 * button in any worktree an agent is working in, and auto-stashing was rejected because a `stash pop`
 * can conflict on its own and would leave a child session's checkout in a state nobody asked for.
 * Committing is the one resolution that is explicit, reversible through git, and leaves the child's
 * work safe.
 *
 * Nothing is called until the operator confirms — cancelling issues no RPC at all.
 *
 * Follows the same hand-rolled overlay pattern as `CreateSessionDialog` (no shadcn Dialog in this
 * app). Keyed by node so re-prompting for a different row starts from that row's own message rather
 * than the last one's.
 */
export function DirtyWorktreeDialog({ prompt, onCommitAndPull, onCancel }: DirtyWorktreeDialogProps) {
  if (!prompt) return null;
  return (
    <DirtyWorktreeDialogBody
      key={prompt.nodeId}
      prompt={prompt}
      onCommitAndPull={onCommitAndPull}
      onCancel={onCancel}
    />
  );
}

function DirtyWorktreeDialogBody({
  prompt,
  onCommitAndPull,
  onCancel,
}: DirtyWorktreeDialogProps & { prompt: DirtyWorktreePrompt }) {
  const [commitMessage, setCommitMessage] = useState(`Save outstanding work on ${prompt.branch}`);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      data-testid="pr-stack-dirty-worktree-dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Uncommitted changes in the worktree"
    >
      <div className="bg-card text-card-foreground border-border flex max-h-[90vh] w-full max-w-lg flex-col gap-3 overflow-hidden rounded-xl border p-4 shadow-lg">
        <h2 className="text-sm font-semibold">Uncommitted changes in this worktree</h2>
        <p className="text-xs text-muted-foreground">
          {prompt.strategy === "rebase" ? "Rebasing onto" : "Merging"} {prompt.baseBranch} would
          touch a worktree that still holds outstanding work. Commit and push it first, or cancel and
          leave the worktree alone.
        </p>
        {/* What is outstanding, before anything is touched — the operator may be looking at a
            checkout a child session's agent is mid-turn in. */}
        <ul
          data-testid="pr-stack-dirty-worktree-paths"
          className="max-h-40 overflow-y-auto rounded-md bg-muted px-3 py-2 font-mono text-xs text-muted-foreground"
        >
          {prompt.dirtyPaths.map((path) => (
            <li key={path} className="truncate">
              {path}
            </li>
          ))}
        </ul>
        <input
          data-testid="pr-stack-dirty-worktree-commit-message-input"
          className="rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-label="Commit message"
          value={commitMessage}
          onChange={(e) => setCommitMessage(e.target.value)}
        />
        <div className="flex justify-end gap-2">
          <Button
            data-testid="pr-stack-dirty-worktree-cancel-btn"
            size="sm"
            variant="outline"
            onClick={onCancel}
          >
            Cancel
          </Button>
          <Button
            data-testid="pr-stack-dirty-worktree-commit-btn"
            size="sm"
            onClick={() => onCommitAndPull(commitMessage)}
          >
            Commit, push and pull
          </Button>
        </div>
      </div>
    </div>
  );
}

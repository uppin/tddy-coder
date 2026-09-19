import type { BranchWorktreeIntent } from "../../lib/branchConflict";
import type { SessionType } from "./createSessionRequest";
import { inputClass, labelClass } from "./createSessionFormStyles";
import type { BaseBranchOption } from "./prstack/baseBranchChoice";

type BranchIntent = BranchWorktreeIntent;

export interface CreateSessionBranchFieldsProps {
  branchIntent: BranchIntent;
  setBranchIntent: (branchIntent: BranchIntent) => void;
  /** `initialValues.baseBranchLabel` — the concrete base named in the "New branch from base" option. */
  baseBranchLabel: string | undefined;
  /** `initialValues.stackParent` — present only for a planned-PR child session. */
  initialStackParent: string | undefined;
  baseBranchOptions: BaseBranchOption[];
  selectedBaseBranch: string;
  setSelectedBaseBranch: (baseBranch: string) => void;
  newBranchName: string;
  setNewBranchName: (newBranchName: string) => void;
  sessionType: SessionType;
  createRemoteBranch: boolean;
  setCreateRemoteBranch: (createRemoteBranch: boolean) => void;
  remoteBranches: string[];
  selectedBranchToWorkOn: string;
  setSelectedBranchToWorkOn: (branch: string) => void;
}

/**
 * Branch mode, base branch, new-branch name (with its Create-Remote-Branch toggle) and the
 * branch-to-work-on picker — the four controls that decide which branch the session's worktree is
 * put on. Presentational: every value stays in `CreateSessionPane`, which submits them.
 */
export function CreateSessionBranchFields({
  branchIntent,
  setBranchIntent,
  baseBranchLabel,
  initialStackParent,
  baseBranchOptions,
  selectedBaseBranch,
  setSelectedBaseBranch,
  newBranchName,
  setNewBranchName,
  sessionType,
  createRemoteBranch,
  setCreateRemoteBranch,
  remoteBranches,
  selectedBranchToWorkOn,
  setSelectedBranchToWorkOn,
}: CreateSessionBranchFieldsProps) {
  return (
    <>
      <div>
        <label className={labelClass} htmlFor="create-session-branch-intent">
          Branch mode
        </label>
        <select
          id="create-session-branch-intent"
          data-testid="create-session-branch-intent-select"
          className={inputClass}
          value={branchIntent}
          onChange={(e) => setBranchIntent(e.target.value as BranchIntent)}
        >
          <option value="new_branch_from_base">
            {`New branch from base${
              baseBranchLabel ? `: ${baseBranchLabel}` : ""
            }`}
          </option>
          <option value="work_on_selected_branch">Work on existing branch</option>
        </select>
      </div>

      {initialStackParent && baseBranchOptions.length > 0 && (
        <div>
          <label className={labelClass} htmlFor="create-session-base-branch">
            Base branch
          </label>
          <select
            id="create-session-base-branch"
            data-testid="create-session-base-branch-select"
            className={inputClass}
            value={selectedBaseBranch}
            onChange={(e) => setSelectedBaseBranch(e.target.value)}
          >
            {baseBranchOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
      )}

      {branchIntent === "new_branch_from_base" && (
        <div>
          <label className={labelClass} htmlFor="create-session-new-branch-name">
            New branch name
          </label>
          <input
            id="create-session-new-branch-name"
            data-testid="create-session-new-branch-name-input"
            type="text"
            className={inputClass}
            value={newBranchName}
            onChange={(e) => setNewBranchName(e.target.value)}
            placeholder="e.g. feature/my-work"
          />
          {/* Only the claude-cli / cursor-cli spawn paths create the worktree in-daemon and can push
              it; a "tool" session spawns tddy-coder, which owns its own worktree — so we don't offer
              the toggle there rather than show a checked box that silently does nothing. */}
          {(sessionType === "claude-cli" || sessionType === "cursor-cli") && (
            <label className="mt-2 flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-create-remote-branch-toggle"
                type="checkbox"
                className="h-4 w-4"
                checked={createRemoteBranch}
                onChange={(e) => setCreateRemoteBranch(e.target.checked)}
              />
              Create Remote Branch
            </label>
          )}
        </div>
      )}

      {branchIntent === "work_on_selected_branch" && (
        <div>
          <label className={labelClass} htmlFor="create-session-branch-to-work-on">
            Branch to work on
          </label>
          <select
            id="create-session-branch-to-work-on"
            data-testid="create-session-branch-to-work-on-select"
            className={inputClass}
            value={selectedBranchToWorkOn}
            onChange={(e) => setSelectedBranchToWorkOn(e.target.value)}
          >
            {remoteBranches.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </div>
      )}
    </>
  );
}

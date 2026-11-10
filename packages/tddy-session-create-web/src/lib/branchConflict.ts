/**
 * Branch-conflict vocabulary: what each answer to the branch-conflict prompt re-submits.
 *
 * When a session creation names a branch another session already owns, the daemon creates nothing and
 * reports `StartSessionResponse.branch_conflict` (see `docs/ft/daemon/session-branch-conflict.md`).
 * The operator then picks one of three ways out, and two of them re-run the same creation with
 * different branch fields. This module is the single place that maps a choice to those fields, so the
 * mapping is unit-testable without mounting the form.
 *
 * No new request shape is invented: "add another agent" is the existing `work_on_selected_branch`
 * intent on the owned branch (which reuses the owner's worktree), and "use a different name" is the
 * same `new_branch_from_base` intent under a name the operator typed.
 */

/** The daemon's `branch_worktree_intent` values the creation form can send. */
export type BranchWorktreeIntent = "new_branch_from_base" | "work_on_selected_branch";

/** The operator's answer to a branch-conflict prompt. */
export type BranchConflictResolution =
  /** Attach to the session that already owns the branch, instead of creating one. */
  | { choice: "switch-to-owner" }
  /** Run a second agent on the owned branch, sharing the owning session's worktree. */
  | { choice: "add-agent" }
  /** Create the session on a different branch, named by the operator. */
  | { choice: "rename"; branchName: string };

/** The `StartSession` branch fields a resolution overrides on the re-submitted request. */
export interface BranchFieldOverrides {
  branchWorktreeIntent: BranchWorktreeIntent;
  newBranchName: string;
  /** Only carried by `work_on_selected_branch`, the one intent that joins an existing branch. */
  selectedBranchToWorkOn?: string;
}

/**
 * The `StartSession` field overrides a resolution re-submits, or `null` when it submits nothing —
 * switching to the owning session is a pure client action (select and attach an existing session), so
 * there is no request to build.
 *
 * @param resolution The operator's choice.
 * @param conflict   The refusal being resolved; its `branch` is the branch the owner holds.
 */
export function startSessionOverridesFor(
  resolution: BranchConflictResolution,
  conflict: { branch: string },
): BranchFieldOverrides | null {
  switch (resolution.choice) {
    case "switch-to-owner":
      return null;
    case "add-agent":
      // Join the owned branch rather than create one: `new_branch_name` must be cleared, or the
      // daemon would read the request as still asking for the branch that just conflicted.
      return {
        branchWorktreeIntent: "work_on_selected_branch",
        selectedBranchToWorkOn: conflict.branch,
        newBranchName: "",
      };
    case "rename":
      return {
        branchWorktreeIntent: "new_branch_from_base",
        newBranchName: resolution.branchName,
      };
  }
}

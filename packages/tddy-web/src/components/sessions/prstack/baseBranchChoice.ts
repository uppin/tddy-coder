import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { prioritiseBaseBranchOptions } from "./prioritiseBaseBranchOptions";
import type { StackNode } from "./stackPlan";

/** One entry of the Start-session dialog's "Base branch" `<select>`: the ref it submits, and its caption. */
export interface BaseBranchOption {
  /** The base ref this option submits as `StartSession.selected_integration_base_ref`. */
  value: string;
  /** What the operator reads. Equal to `value` except for the project default of a legacy project. */
  label: string;
}

/** The whole state of the "Base branch" `<select>`: what it offers, and what it starts on. */
export interface BaseBranchChoice {
  options: BaseBranchOption[];
  selected: string;
}

/** Caption for the project default of a legacy project, whose stored default branch ref is empty. */
const PROJECT_DEFAULT_LABEL = "project default";

/**
 * The "Base branch" `<select>`'s option list *and* its pre-selection, resolved together.
 *
 * The two used to be computed independently — the options from {@link prioritiseBaseBranchOptions}
 * (the stack's own branches) and the pre-selection as `options[0]` — while the dialog's
 * "New branch from base: <x>" caption came from {@link deriveStackBaseBranch}. They drifted for a
 * planned PR repointed onto the project default branch: `RepointPlannedPr` drops every parent edge, so
 * the caption read `origin/master` while the picker pre-selected (and submitted) an unrelated stack
 * branch, silently undoing the repoint. Resolving both here from the same derived base is what keeps
 * them from drifting again.
 *
 * The rules, and why each holds:
 *
 * 1. `selected` is the node's derived stack base — the same value the caption states — never whichever
 *    option the ordering happens to put first.
 * 2. The project default branch is always offered, appended after the stack's own branches: a node
 *    based onto it must be *showable* as such, and re-pickable. Last, because a node in a stack
 *    normally chains onto a predecessor; the default is the deliberate escape from the stack.
 * 3. `selected` is therefore always one of `options`, so nothing can be submitted that the operator
 *    cannot see selected. That holds structurally rather than by clamping: `deriveStackBaseBranch`
 *    returns either an ancestor's branch — which `prioritiseBaseBranchOptions` also carries, including
 *    a transitive ancestor reached past a merged parent, as an "other" stack branch — or the default
 *    branch this appends.
 * 4. When the stack offers no branch of its own there is nothing to choose: no options and no
 *    selection, so the dialog hides the picker and the daemon resolves the base itself from an empty
 *    `selected_integration_base_ref` (`select_worktree_base_ref` falls through to chain resolution).
 *    This is the lone-planned-root behavior, preserved rather than replaced by a default-only picker.
 * 5. A legacy project stores no `main_branch_ref`, so `defaultBranch` may be empty. That empty ref —
 *    the one the daemon resolves for itself — is still offered and still selectable, under a label
 *    naming it instead of an option that reads blank.
 *
 * `defaultBranch` is the project's `main_branch_ref` as stored (a remote-tracking ref, `origin/master`)
 * while a node's `branch` is a local name; this layer keeps both as given and leaves lifting them into
 * one form to its caller.
 */
export function baseBranchChoice(
  node: StackNode,
  nodes: StackNode[],
  defaultBranch: string,
): BaseBranchChoice {
  const stackBranches = prioritiseBaseBranchOptions(node, nodes);
  if (stackBranches.length === 0) return { options: [], selected: "" };

  const options: BaseBranchOption[] = stackBranches.map((branch) => ({
    value: branch,
    label: branch,
  }));
  // A stack branch that *is* the default branch is already offered; listing it twice would let the
  // same ref be selected under two options. Compared as given, so this catches a collision only when
  // both sides name the branch the same way — a stack branch is a local name while `defaultBranch` is
  // normally remote-tracking, and reconciling the two here would mean threading the project's remote
  // into a resolver that otherwise never needs it.
  //
  // A pr-stack node owning the default branch is pathological anyway: nodes create feature branches.
  if (!stackBranches.includes(defaultBranch)) {
    options.push({ value: defaultBranch, label: defaultBranch || PROJECT_DEFAULT_LABEL });
  }

  return { options, selected: deriveStackBaseBranch(node, nodes, defaultBranch) };
}

import type { BranchResolution } from "../../../gen/connection_pb";

/**
 * How a planned-PR row states its branch's standing against its base.
 *
 * A discriminated view rather than the wire message's booleans, because two states are byte-identical
 * to a healthy branch on every field that carries a number:
 *
 * - **A comparison that could not be made is not "clean".** It arrives with nothing behind and no
 *   conflicts, so only its own `unavailable` flag tells it apart from a branch that is genuinely up
 *   to date (D27, the same rule D12 already imposes on PR status).
 * - **A comparison that has not arrived is not "clean" either.** An unanswered poll and a daemon
 *   that predates base sync are both `unknown`, and a row says nothing it does not know.
 */
export type BaseSyncView =
  /** No resolution yet, or a daemon that reports no comparison. The row renders nothing. */
  | { kind: "unknown" }
  /** The comparison could not be made, with the daemon's operator-facing reason. */
  | { kind: "unavailable"; reason: string }
  /** Taking the base's commits would conflict, in these paths. */
  | { kind: "conflicts"; baseBranch: string; paths: string[] }
  /** The base has `behind` commits the branch lacks, and they merge cleanly. */
  | { kind: "behind"; baseBranch: string; behind: number }
  /** The branch contains every commit on its base. A badge, not silence. */
  | { kind: "in-sync"; baseBranch: string };

/** The one state a pull is offered from — see {@link canPullFromBase}. */
export type BehindBaseSyncView = Extract<BaseSyncView, { kind: "behind" }>;

/**
 * Read a branch resolution's base comparison as the state the row renders.
 *
 * The base named is always the one the daemon actually resolved and compared (`baseSync.baseBranch`),
 * never the base the row planned: the counts are meaningless next to a ref they did not come from
 * (D28).
 */
export function baseSyncView(resolution: BranchResolution | undefined): BaseSyncView {
  const baseSync = resolution?.baseSync;
  if (!baseSync) return { kind: "unknown" };
  // Checked before the counts, not after: a failed comparison reports nothing behind and no
  // conflicts, so any clause that reads those first would render it as clean.
  if (baseSync.unavailable) return { kind: "unavailable", reason: baseSync.unavailableReason };
  // The behind count must not be what decides whether the operator is told about a conflict.
  if (baseSync.hasConflicts) {
    return {
      kind: "conflicts",
      baseBranch: baseSync.baseBranch,
      paths: baseSync.conflictedPaths,
    };
  }
  if (baseSync.behindCount > 0) {
    return { kind: "behind", baseBranch: baseSync.baseBranch, behind: baseSync.behindCount };
  }
  return { kind: "in-sync", baseBranch: baseSync.baseBranch };
}

/**
 * Whether the row offers to pull its base in.
 *
 * Only a clean behind-count is pullable. In sync there is nothing to take and a zero-commit merge
 * still runs a git operation; on conflicts the pull would abort; and an action derived from a
 * comparison that was never made is an action derived from nothing.
 *
 * A type predicate rather than a plain boolean, so the caller that passes the gate also holds the
 * base and the count the control has to name — the pull cannot be issued against a base no
 * comparison produced.
 */
export function canPullFromBase(view: BaseSyncView): view is BehindBaseSyncView {
  return view.kind === "behind";
}

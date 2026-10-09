# 2026-10-09 — the pre-apply compile gate does not check the third crates a cross-crate move re-points

**Category:** Future enhancement (engine defect, attribution)
**Source:** #reshape 10/19 (`apply-robust`), found while widening the baseline to the destination

## What remains

`refuse_a_broken_baseline` (`packages/tddy-code-restructuring/src/runner/compile_gate.rs`) checks the
packages owning the plan's snapshot and anchor files and, since `#reshape` 10/19, each cross-crate
move's destination. A crate move also re-points callers in **other** crates that depend on the origin.
Those crates are checked by the post-apply gate (`refuse_a_broken_result` reads every completed edit's
paths), but not by the baseline. So a consumer crate that was already broken before the plan ran is
reported afterwards as `N of M operation(s) were applied, and the tree no longer compiles`, and
blamed on the plan. This is the same misattribution the 2026-09-25 entry recorded for the destination.

## Why it was deferred

Which crates a move re-points is known only after the operation resolves (the reference survey).
Checking every reverse dependency of the origin up front would make the baseline as expensive as a
workspace check on a crate like `tddy-daemon`, and no run has hit this yet.

## What would close it

Either run the survey before the baseline and add the crates it names, or, when the post-apply gate
fails in a crate the baseline did not cover, re-check that crate at `HEAD` (`git stash`-free, in a
temporary worktree) and say "already broken before the plan" instead of blaming the run.

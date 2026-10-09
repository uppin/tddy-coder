# 2026-10-09 — a failed run leaves no copy of the plan as it was before the run

**Category:** Future enhancement (engine ergonomics)
**Source:** #reshape 10/19 (`apply-robust`), leftovers item 4

## What remains

`apply` writes the plan back after every committed operation (`record_applied_op`), so after a run
that failed and was rolled back by hand, the plan's anchors describe the tree the run left. Since
`#reshape` 10/19 the `StaleOperation` refusal says so and tells the author to regenerate the plan.
Regenerating means re-running `restructure anchors` for every operation, although the plan as it was
before the run would be exactly right for the restored tree.

## Why it was deferred

The backlog entry asked only for the message. Keeping a copy is a new file under `.restructure/`
with its own lifetime questions: when it is replaced, whether a resumed run keeps the first copy,
and what `status` shows. None of that was needed to stop the misleading refusal.

## What would close it

At `open_run`, after `.restructure/<plan>-<digest>/` exists, write the plan's text as
`plan.before-run.jsonl` unless a copy for this journal already exists. The stale refusal then names
it: "restore it with `cp <state dir>/plan.before-run.jsonl <plan>`". Test: fail a run at the compile
gate, roll it back with git, restore the copy, and apply 1 of 1.

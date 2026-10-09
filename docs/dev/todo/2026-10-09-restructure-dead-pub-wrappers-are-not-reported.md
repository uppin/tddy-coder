# 2026-10-09 — a `pub` wrapper that loses its last caller in a restructure run is not reported

**Category:** Future enhancement (missing report; nothing breaks, dead code is left behind)
**Source:** #reshape 6/19 (`feature/reshape/methods-leave-type`). Narrowed out of
`2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md`
("then report any wrapper with no caller left"), which that node resolves and deletes.

## What is covered already

- **Private wrappers in touched files.** After an apply, the tidy prints
  `warning remains: <file>:<line>: function `x` is never used` for every rustc warning still reported
  in a file the run touched (`packages/tddy-code-restructuring/src/runner/tidy.rs`,
  `report_remaining_warnings`). That covers `#carve` 17's two dead host wrappers
  (`mint_first_admission_token`, `session_dir_for`), because both were private.
- **Delegators.** `retarget_impl` with `leave_delegator` notes each delegator that has no caller in the
  workspace, using the reference set it already requests (#reshape 6/19).

## What is missing

Neither of the following is ever reported:

- a `pub` method of a library crate whose last in-workspace caller a run re-pointed (`repoint_call`'s
  bulk form, `read_fields_through` followed by `extract_method`). rustc never warns about `pub` items;
- a wrapper in a file the run did not touch.

Closing it would need a workspace reference survey after the run, of every function whose callers the
run edited. That means one `textDocument/references` request per such function, on a tree that has
changed, so either a second server pass or the warm index.

## Why deferred

- It costs a reference survey on the post-apply tree, which no operation does today.
- A `pub` item with no in-workspace caller may be public API, so the report has to say "no caller in
  the workspace" and leave the judgment to the author. That is a wording and policy decision of its
  own.
- The cases that actually occurred in `#carve` 17 are already reported by the tidy.

# oversized-file: changeset.rs — two unrelated data models

**Location:** `packages/tddy-core/src/changeset.rs`
**Category:** oversized-file
**Detected:** 2026-09-15 by structural audit
**Metrics:** **964 production lines** (1,627 total) · 2 unrelated data models · budget 500
**Restructure:** required — `extract_module --to_file` × 4
**Status:** Open — claimed by #491, in flight
**Claimed by:** #491 — `#carve` 5/10 `core-foundations` · draft · `feature/carve/core-foundations`
**Lands after:** #488, #489, #490, #498

## Measurement history

| Run | Production lines | Models | Note |
|---|---|---|---|
| 2026-09-15 | 964 | 2 | first detection |

## What the tool found

Production lines counted to the first `#[cfg(test)]`. The file holds **two data models that share
nothing**:

| Lines | Model |
|---|---|
| 44–277 | **PR-stack**: `Stack`, `StackNode`, `impl StackNode`, `impl Stack` |
| 278+ | **session**: `Changeset`, `ChangesetState`, `ChangesetWorkflow`, `GithubPrStatus`, … |

plus file I/O (`read_changeset`, `write_changeset`, `write_changeset_atomic`) and context-merge
functions. Nothing in the first model names the second.

## Why it matters here

Two consumers want different halves and both get all of it. `tddy-workflow-recipes`'
`pr_stack/mod.rs` references `changeset::Stack` **15 times**, `read_changeset` 7 and
`update_stack_atomic` 3 — and it is in a crate that has nothing to do with the session model. Until
`Stack` is its own module there is no seam to extract a `tddy-pr-stack` along.

## What would close it

`changeset/{stack,model,io,merge}.rs` behind a glob facade, so every existing `changeset::` path
resolves and no consumer is edited. Moving a whole `impl` is free of caller churn — a method is
reached through its type — so `impl Stack` and `impl StackNode` carry no rewrite cost.

`/code-restructuring` job. Target: parent under 200 production lines, no module over 400.

## If you are about to change this code

#491 splits the file **in place** behind a facade: no public path changes and no behaviour changes.
Reading or calling `changeset::` anything is unaffected.

Coordinate if you are **adding a field** to `Changeset` or `Stack`, or a new free function to this
file — it lands in whichever module #491 moved that neighbourhood into, so a change planned against
the flat file will conflict textually even though it does not conflict semantically.

## Verified by hand

2026-09-15: opened the file and confirmed the boundaries — `pub struct Stack` at 44,
`pub struct StackNode` at 53, `impl StackNode` at 91, `impl Stack` at 101, `pub struct Changeset` at
278, `ChangesetState` at 352, `ChangesetWorkflow` at 417. Confirmed no `crate::worktree` path in
production (the apparent one is a doc-comment link at 473).

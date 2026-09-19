# stale: the apply loop opts out of plan-scoped restructure state

**Location:** `packages/tddy-index-daemon/src/apply.rs:45` `StatePaths::under(root)`
**Category:** stale
**Detected:** 2026-09-18 during `#carve` 3/9's green phase ([#490](https://github.com/uppin/tddy-coder/pull/490))
**Metrics:** 1 call site · 1 line to change · 4 doc comments + 1 committed package doc to reconcile
**Restructure:** no — a one-line call change, not a seam
**Status:** Open
**Claimed by:** nobody

`#490` made `tddy-code-restructuring`'s run state **plan-scoped**: `StatePaths::for_plan(root, plan)`
puts the journal and ledger in `<root>/.restructure/<plan stem>-<digest>`, so a completed plan no
longer blocks the next one under the same root and `--resume` resumes the plan it was given. Both CLI
entry points — `restructure apply` and `restructure status` — use it.

**This crate's own apply loop does not.** It still calls `StatePaths::under(root)`, so a plan applied
**through the index daemon** keeps the single repo-scoped journal and still pays the tax:

    a journal already exists for this plan — pass `--resume`

on a plan that has never run, because a *different* plan completed under that root.

## What would close it

One line, and the plan path needs no threading — `options.plan()?` is already read on the line above:

```rust
let paths = StatePaths::for_plan(root, options.plan()?)?;
```

What makes it more than one line is the reasoning that has to change with it. The per-root queue is
justified, in four places, partly by the journal carrying no plan identity:

| Where | What it says |
|---|---|
| `apply.rs` `apply_plan` doc | "the journal this writes carries no plan identity, so two runs under one root must not be in here at once" |
| `index.rs:8`, `:43`, `:121` | the queue exists because "`.restructure/journal.jsonl` is keyed by root with no lock file" |
| `operations.rs:16`, `queries.rs:8` | operations touching `.restructure/` take the root's queue first |
| `docs/code-index-service.md:19` | same rationale, and **committed package docs — changeset workflow only, never edited directly** |

## Why it was left

Nothing is broken meanwhile: the per-root queue serializes access to the **tree** as well as to the
journal, so after this change it is merely stricter than it needs to be. Correctness never depended on
the repo-scoped layout.

And `#490`'s `## Boundaries` scope it to the restructuring tool. Folding a behaviour change to this
crate plus a changeset for its published docs into that node would have mixed a second package's
concurrency story into a diff that is already the largest in its stack.

## Related

The tool-side record, `blocking-nested-and-cluster-moves.md`, was **closed and deleted** when #490
wrapped: all four refusals it tracked are gone, and its final measurement (2 of 2 entangled modules
moved in one operation, `cargo check` clean) is in
`packages/tddy-code-restructuring/docs/changesets/2026-09-19-cluster-moves-and-plan-scoped-state.md`.

This record is the remainder, and it is **the last consumer on the repository-scoped layout** — which
is why it is filed here, in the package that owns the call site, rather than left as a footnote on a
closed record in another package.

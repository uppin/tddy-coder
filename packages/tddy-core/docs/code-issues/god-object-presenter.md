# god-object: Presenter

**Location:** `packages/tddy-core/src/presenter/presenter_impl.rs` — `Presenter`
**Category:** god-object
**Detected:** 2026-09-15 by structural audit
**Metrics:** **37 fields** · **46 methods** in one `impl` · 1,788 production lines · budget 500
**Restructure:** required — field regrouping (hand-written), then `extract_module --to_file` × 6
**Status:** Open — claimed by #491 and #495, in flight
**Claimed by:** #491 — `#carve` 5/10 `core-foundations` (the 37 **fields** → five sub-structs) · draft
**Claimed by:** #495 — `#carve` 9/10 `presenter-split` (the 46 **methods** → six modules) · draft
**Lands after:** #488, #489, #490, #498 (for #491); + #491, #492, #493, #494 (for #495)

## Measurement history

| Run | Fields | Methods | Production lines | Note |
|---|---|---|---|---|
| 2026-09-15 | 37 | 46 | 1,788 | first detection |

## What the tool found

One struct with 37 fields; one `impl Presenter` from line 152 with 46 methods. The field groups are
**already named by the struct's own doc comments** — the agent-activity provenance set, the three
external-surface channels, the fields that exist only for the deferred-start path — but nothing
expresses them in the type system.

## Why it matters here

Every method can reach every field, so no method's dependencies are visible from its signature. The
methods are not even in cohesion order: the three `broadcast*` methods sit at positions 8, 12 and 18,
separated by activity and plan-review methods; backend selection runs 15–22 then picks up 45–46 at
the far end. A reader cannot tell which state a method touches without reading it.

## What would close it

Two changes that must happen in that order:

1. **Fields → five owned sub-structs** (`WorkflowRun` 12, `PendingQuestions` 4, `ActivityRecorder` 6,
   `ViewChannels` 5, `BackendSelection` 8), leaving `state` and `tddy_data_dir` on `Presenter`.
   Hand-written: grouping fields into owned types is a type-level change no assist expresses.
2. **Methods → six modules following those boundaries.** This needs a preparatory hand edit: the plan
   schema records that `extract_module` **refuses** to lift one member out of an `impl` its siblings
   call, the refusal fires before the assist runs, and no ordering fixes it because an `impl` body
   cannot hold a `mod`. Partition the single `impl` into six `impl Presenter` blocks by hand first,
   then move each whole block — **the cheapest move Rust has**, free of caller churn.

## If you are about to change this code

This is the most disruptive claim in the crate, and the two PRs land far apart.

- **Adding a method**: #495 must place it in one of six modules, and #491's field grouping will not
  have seen the fields it touches. Cheap for you, real cost for them.
- **Adding a field**: worse — #491 has to decide which sub-struct owns it, which is a design call
  it made deliberately from the existing doc comments.
- **Reading state**: unaffected by #491 (field access sites move but paths do not) and unaffected by
  #495 (a method is reached through its type).

If your change is confined to `presenter/workflow_runner.rs` (1,015 production lines, a separate
file), **neither node touches it** — that is often the cheapest way through.

## Verified by hand

2026-09-15: counted the struct declaration (lines 55–136) and the production `impl` separately. An
earlier pass reported **44 fields and 79 methods** and was **wrong on both**: the fields were
eyeballed rather than counted, and the method `grep -c` ran over the whole file, including the test
module. Measured on production lines only: **37 and 46**. The 1,788-line figure was correct.

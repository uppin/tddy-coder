# 2026-10-04 — The plan dialog's Run always starts the plan from the beginning

**Category:** Deferred feature
**Source:** `#live-plan` 13/15, [#572](https://github.com/uppin/tddy-coder/pull/572), `RunPlan`'s mapping onto `code_index.Apply`.

## What is left

`code_navigation.RunPlan` sends `code_index.Apply` with `resume: false`, no `from` and no `stop_after`.
After a run that stopped part-way — a failed operation, a group that did not compile — the journal
holds the operations that did complete, and running the same plan again from the start will most
likely be refused rather than continued. The dialog offers no way to resume, to start from a given
operation or to stop after one.

## Why it was left

Which behaviour a second Run should have is a product decision this node did not make: resume the
journal, restart from the first operation that did not complete, or ask the operator. It also depends on
how the index daemon treats a journal that already records completed operations, which is the apply
engine's contract and not the dialog's. `RunPlan`'s request has no field for any of the three, so
adding one changes the wire contract too.

## What would close it

A decision on resume and `from` semantics for a re-run, then `RunPlanRequest` fields (or a derived
choice) mapped onto `ApplyRequest`, and the dialog's Run labelled for what it will do. The site is
the `resume: false` in `run_plan` in `packages/tddy-daemon-rpc/src/code_navigation.rs`.

# 2026-09-19 — A mutually-referencing cluster of modules moves as one unit

**Type:** Feature

`#carve` 3/10. `move_module_to_crate` modelled one module, so the subsystems worth extracting —
which are almost never a single module — could not move: the header pass re-pointed every `crate::`
path at the origin, making a co-moving sibling reference a destination→origin edge, and the
reference survey ran against a tree where the siblings had not moved. `#unbundle` node 3 moved **0
of 4** entangled modules for exactly this reason and hand-moved all four.

`tddy-code-restructuring` gains `move_cluster_to_crate`, which resolves a set into one edit, and
keys `.restructure/` run state by the plan rather than by the repository. `tddy-index-daemon` maps
the one new error variant. Details and the closing measurements are in each package's own entry.

Two lessons worth carrying, both found by doing rather than by review:

**A live apply is not optional for this operation.** 380 unit tests passed against a cluster path
that no plan could reach, and then against a co-moving rewrite that emitted `use destination::…`
*inside* the destination crate — `E0433`, since a crate cannot name itself. Both were found only by
running the tool for real and compiling the result. The acceptance suites end in `cargo check` for
this reason, and the new `cluster_move_acceptance` does too.

**`extract_module` cannot see sibling seams cut by the same plan.** Carving `crate_move.rs` into
eight modules in one plan passed `check --deep` with `no findings`, reported 8 of 8 applied, and left
a tree that did not compile: 13 errors needing 6 import- and visibility-only fixes. The reference
survey, the import pass and the visibility restoration all read "outside the new module" as "the
parent file". Recorded in `docs/dev/todo/`; it bites every multi-seam plan, which is what each
remaining `#carve` node writes.

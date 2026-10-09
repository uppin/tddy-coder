# 2026-10-09 — no restructure operation introduces a parameter struct

**Category:** Future enhancement (missing engine operation)
**Source:** #reshape 16/19 (`fn-sizes-rest`), found while planning the `apply_held_plan` cut

`extract_method` takes as parameters every local the range reads or writes. A long function whose phases all thread the
same state (`apply_held_plan` in `runner/entry_points/store_run.rs`: nine locals) cannot be cut without functions of 8+
parameters, which `clippy::too_many_arguments` rejects under `-D warnings`. `#reshape` 4 only notes that arity. The
vocabulary has `convert_tuple_return_to_struct`, which groups a return value, but nothing that groups parameters or
locals. Keeping a struct whole instead of destructuring it is not an operation either. rust-analyzer has
`destructure_struct_binding`, the inverse.

The candidate operation: given a function and a list of its locals (or parameters), declare a struct holding them, bind
one value of it where the first of them is bound, and rewrite each use `x` to `state.x`. It would be checked by the
compile gate like every other operation. Each later `extract_method` would then take `&mut state` as one parameter.

## Why deferred

It is a new engine feature, and `#reshape` 16 is a size node. `apply_held_plan` was grouped by hand with consent instead
(`2026-10-09-apply-held-plan-run-state-was-grouped-by-hand.md`). `#reshape` 19 (`fn-sizes-backend`) is likely to meet the
same shape in `backends/rust.rs` (`resolve_opening`, `assisted_edit`). That is the evidence for whether the operation
earns its cost.

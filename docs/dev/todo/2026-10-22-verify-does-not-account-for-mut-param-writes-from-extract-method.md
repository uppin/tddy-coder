# 2026-10-22 — `restructure verify` does not account for `&mut`-parameter writes made by `extract_method`

**Category:** Restructure tooling gap
**Found in:** `#reshape` 18/24 (`fn-sizes-rest`), plan 01 (`items_of_module`, `mod` arm)
**Owner:** the `#reshape` sweep node

## What happens

When an `extract_method` range writes a local that the enclosing function reads later, the new function takes it as a
`&mut` parameter, and the engine rewrites the assignment inside the new function: `open_child = Some(..);` becomes
`*open_child = Some(..);`. The apply compiles and is behaviour-preserving, but `restructure verify --against HEAD`
reports the old statement as lost and the deref statement as gained, and the run fails with "1 statement(s) the tree
lost and 3 it gained". `verify` has no option that declares this (its options are `--retarget`, `--repoint`,
`--rebind`, `--detach`).

## Accepted for now

Developer decision, 2026-10-22: accept the pair by hand review of the diff, and record it in each cut's commit message.
Commit `2782c4b53` (`items_of_module`) is the first use.

Developer decision, 2026-10-22 (second): accept the arm lines of a match-arm body extraction. When `extract_method` moves
an arm's body out, the arm's own lines (for example `X => Options {`, `X => {`, `},`) are replaced by the call site
(`X => f(x),`); `verify` reports them as lost and the call as gained. Accepted by hand review of the diff, listed in the
cut's commit message. The `options_for` cut (`restructure_args.rs`) is the first use.

Developer decision, 2026-10-22 (third): accept the whole `&mut` family. Every mechanical rewrite forced by a local becoming
a `&mut` parameter in an `extract_method` cut is accepted by hand review of the diff, and listed in the cut's commit message:
- `x = …` → `*x = …`;
- `&mut x` → `x` (the moved call passes the `&mut` parameter, not a fresh borrow);
- autoderef'd field or method access left unchanged;
- `&x` → `&*x` or `x`.

The `check_plan` rehearsal cut (`rehearse_op`, `check_entry_points.rs`) is the first use of the third rule.

Any other unaccounted statement still stops the run.

## Wanted

`verify` accounts for the rewrite of a written local into `*param` when the new function's parameter is `&mut` and the
range is the source of that parameter, so that a cut of this shape needs no hand note.

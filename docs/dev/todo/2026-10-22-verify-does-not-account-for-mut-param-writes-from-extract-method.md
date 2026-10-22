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
Commit `2782c4b53` (`items_of_module`) is the first use. Any other unaccounted statement still stops the run.

## Wanted

`verify` accounts for the rewrite of a written local into `*param` when the new function's parameter is `&mut` and the
range is the source of that parameter, so that a cut of this shape needs no hand note.

# 2026-10-21 — `detach_method` turns an `impl`'s members into free functions

`#reshape` 15/24. One plan line turns a run of members of an inherent `impl T` into free functions over a
named parameter, in the same module: `&mut self` becomes `name: &mut T`, `self` and `Self` in the body become
`name` and `T`, and every caller the language server reports is re-pointed (`r.m(a)` to `m(&mut r, a)`,
`Self::m` to `m`, a call from another module through its module path, with a `use` added when needed). A block
the run empties is removed. By-value receivers, generic blocks, trait members, `pub` members, associated
consts, shadowed names, clashing functions, foreign receiver types and calls inside macros are refused before
anything is written, naming the file and line. `restructure verify --detach TYPE=NAME` accounts for the run.
See [`detach_method`](../rust-code-restructuring.md#detach_method).

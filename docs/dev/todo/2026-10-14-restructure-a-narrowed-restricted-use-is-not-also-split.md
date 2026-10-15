# 2026-10-14 — a restricted grouped `use` that must narrow its restriction and split is only narrowed

**Category:** Restructure engine limit (crate moves, grouped `use`)
**Source:** `#reshape` 7/19 (`feature/reshape/move-grouped-use`, PR #605), deferred from its scope

## What the engine does

`crate_move::header::rewrite::rewrite_of` returns the narrowing edit as soon as any reach of a statement is `narrowed`
(`pub(in crate::x)` naming a module that stays becomes `pub(crate)`, rule R8). It never goes on to ask whether the
statement's members also disagree about where they land (Rule S). For

```rust
pub(in crate::x) use crate::{a, b};   // `x` stays; `a` moves to the destination, `b` stays
```

the restriction is respelled and the group is left as written, so the `use` still names `a` through its old path. The two
edits address overlapping bytes of one statement (the restriction in front, the group behind it), which is why they cannot
simply both be returned from `rewrite_of`: they would have to be one edit that writes the whole statement, narrowed and split.

## Why deferred

No acceptance test, no `#carve` input and no recorded refusal has this shape: a restricted re-export whose members split.
Reproduced from reading the code, not from a failing run — there is no test for it, in either direction. `#reshape` 7/19
owns `pub(in …)` respelling and the split separately (tests 10–12 and 1–4), and both are covered one at a time.

## What would close it

`rewrite_of` with a narrowed reach and members that do not agree writes one statement edit: the restriction respelled as
`pub(crate)`, the kept members in place, each lifted member as a statement of its own with the same restriction. A test
pins it in `tests/grouped_use_crate_move.rs`, and `check` reports the same statement as the apply rewrites (parity).

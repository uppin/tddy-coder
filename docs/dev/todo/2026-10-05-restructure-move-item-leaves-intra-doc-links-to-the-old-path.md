# 2026-10-05 — `restructure` `move_item` re-points code but not the intra-doc links that name the moved item

**Category:** Future enhancement (missing capability; nothing breaks the build, `cargo doc` would warn)
**Source:** #carve 17/21 (PR #532) E1-E5

## What I ran

`move_item` of `svc_spawn_split_agent::split_forward_deadline` to `connection_service::agent_roster`
(`reexport: outside`). `check --deep`: `no findings`. `apply`: `applied 1 of 1`, compile gate clean,
`restructure verify` "every statement accounted for".

## What was left

`handler_state.rs:106` still reads

```rust
/// How long to wait ... (see
/// [`svc_spawn_split_agent::split_forward_deadline`]), under this host's config.
```

an intra-doc link to a path that no longer exists. The engine re-points every code reference the server reports
and no comment text; `verify` does not read comments. I did not edit it: after an apply a hand edit is a build
correction only, and a broken doc link is not one. Two earlier stages of this carve fixed such links by hand
(`03941096`).

## What would make it unnecessary

`move_item` / `reparent_module` rewriting a `[`path`]` link in a `///` or `//!` line whose path resolves (by
`textDocument/references`, which rust-analyzer reports for doc links) to the moved item, leaving the rest of
the line byte for byte; or at least listing each such link in the apply's notes so it is not found by a later
`cargo doc`.

## Minimal reproduction

```rust
// a.rs
pub fn f() {}
// b.rs
/// See [`crate::a::f`].
pub fn g() {}
// plan: move_item a::f -> c   =>   b.rs keeps `[`crate::a::f`]`
```

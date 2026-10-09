# 2026-10-09 — the Rust backend's trait impls pin every operation to the crate of `RustBackend`

**Category:** Future enhancement (crate-split prerequisite; nothing breaks while there is one crate)
**Source:** #reshape 18/19 (`feature/reshape/backend-session`), decision F3. Motivation:
`2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md` (stack 2).

## What #reshape 18 left

After #reshape 18, every operation of the Rust backend is a free function over `session: &mut RustBackend`. `RustBackend`
keeps inherent members only in `backends/rust.rs` (construction) and the session modules `transport`, `readiness`,
`documents`, `references`. Free functions can move to any crate above the type's crate. The type split itself was not
built.

## What still blocks the split

- `impl LanguageBackend for RustBackend` (in `backends/rust/language_backend.rs` after #reshape 17) can only be written in
  the crate of `RustBackend` or of `LanguageBackend` (orphan rule). `LanguageBackend` lives in the model crate, the bottom
  of the proposed split.
- Its `resolve` reaches every operation through `resolve_opening`: `crate_move::resolve`, `move_items`, `reparent_module`,
  `retarget_impl`, `repoint_call`, `repoint_facade_imports`, `resolve_cluster`, the assists, `detach_method`, and so on.
  `ItemResolver` / `ItemAtResolver` (in `item_path.rs`) reach the item-path functions the same way.
- So the crate that holds `RustBackend` (the session, crate 3 "rust-support" in the proposed split) would also hold the
  dispatcher, which depends on crates 4 and 5. That is a cycle.

## Options

1. **Split the type.** `RustSession` holds today's fields and the session modules' methods, in the Rust-support crate.
   `RustBackend { session: RustSession }`, in the top Rust crate, holds the construction surface, the trait impls and the
   dispatcher. The detached functions take `&mut RustSession`. Cost: the new struct and its forwarding constructors are
   hand-written wiring, about 80 lines. Every `session: &mut RustBackend` parameter changes type, which an engine
   `rename_symbol` on the struct does in one line. But that rename also edits the 7 consumer files and the public
   re-export, which then have to go back to `RustBackend`.
2. **A dispatch table.** `RustBackend` stays in the bottom crate and holds `&'static [(RefactorKind, fn(&mut RustBackend,
   &RefactorOp, &Workspace) -> Result<Resolution>)]`, injected by the top crate at construction. This adds no new type, but
   the registry's wiring changes and dispatch becomes data.

## Also needed when the functions leave the crate

The detached functions read session fields directly in about 15 places: `progress` (11), `trace`, `indexed`, `chatter`,
`environment`, `wait_heartbeat`, `unresolved_token`. Across a crate boundary they need accessors, or the fields widened.
#reshape 7's narrowest-visibility widening would make them `pub`, which exposes session internals.

## Why deferred

- Either option is hand-written wiring, which #reshape's engine-only rule does not cover without consent.
- The right shape depends on stack 2's crate boundaries, which stack 2 decides.
- While there is one crate, the seam is pure indirection.

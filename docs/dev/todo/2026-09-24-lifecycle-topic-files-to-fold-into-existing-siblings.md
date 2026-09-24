# 2026-09-24 — four `connection_service` topic files belong inside siblings that already exist

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
("Consent list" item 6)

## Why deferred

**Needs developer consent, because of an engine gap.** The discovery's seams for
`connection_service.rs` sent four clusters into files that **already existed** beside it. The
engine's `extract_module` refuses to write into an existing module: its name-collision check fails
the operation. So plan `01` wrote each cluster into a new file of its own (`0792dc29`), and folding
it into its sibling is a hand edit. The developer's rule for the run (2026-09-24) was no hand move
around a refusal.

| New file (`src/connection_service/`) | Lines | Holds | Belongs in |
|---|---:|---|---|
| `stack_child_spawn.rs` | 35 | `StackChildSpawnHandler` (the struct) | `child_spawn_handler.rs`, which already holds its `impl` |
| `conversation_spawn.rs` | 64 | `recipe_enables_conversation_spawn`, `conversation_branch_slug`, `GrillMeConversationSpawnHandler` | `conversation_spawn_handler.rs` |
| `roster_replacement.rs` | 24 | `roster_replacement_pairs` | `agent_roster.rs` |
| `stack_seed_validation.rs` | 104 | `validate_stack_seed_base_session` (public), `session_repo_is_in_project` | `stack_parent.rs` |

The struct is in one file and its only `impl` in the next:

```rust
// stack_child_spawn.rs
pub(crate) struct StackChildSpawnHandler {
    pub(crate) stack_parent_host: Arc<dyn StackParentHost>,
    …
}

// child_spawn_handler.rs
impl tddy_core::toolcall::ChildSpawnHandler for StackChildSpawnHandler { … }
```

`conversation_spawn_handler.rs` likewise holds `GrillMeConversationSpawnHandler`'s `impl`. And
`connection_service.rs` carries a `mod` and a glob re-export for each:

```rust
// connection_service.rs:255–276, 416–417
mod stack_child_spawn;
pub(crate) use stack_child_spawn::*;
mod conversation_spawn;
pub(crate) use conversation_spawn::*;
mod roster_replacement;
pub use roster_replacement::*;           // public: `roster_replacement_pairs` is crate surface
mod stack_seed_validation;
pub use stack_seed_validation::*;        // public
```

## What would close it

With the developer's consent, by hand: append each file's items to its sibling, merge the `use`
lines, delete the file and its `mod` line. The `pub use` re-exports move to the sibling's `mod`, so
`tddy_session_lifecycle::connection_service::roster_replacement_pairs` and `validate_stack_seed_base_session`
keep their public paths. Then the destructure's baseline (61 targets, 622 / 22 / 1). Or an
`extract_module` mode that appends to an existing module file; none exists.

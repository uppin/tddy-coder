# 2026-10-05 — `restructure` `move_item` re-points a caller with a full `crate::…::item` path where it could add an import

**Category:** Future enhancement (style; behaviour is correct)
**Source:** #carve 17/21 (PR #532) E1-E5

## What happened

E3 (`move_item` `split_forward_deadline` to `connection_service::agent_roster`, `reexport: outside`) re-pointed its two
callers. Both were written as `svc_spawn_split_agent::split_forward_deadline(&self.config)` after a
`use ...::svc_spawn_split_agent;`. The engine removed the now-unused `use` and wrote the call as
`crate::connection_service::agent_roster::split_forward_deadline(&self.config)` (`handler_state.rs`,
`svc_provision_agent_clone.rs`). E4 did the same in `conversation_worktree_op.rs`
(`crate::connection_service::peer_session_answer::resolve_worktree_root_in_session_dir(...)`). `reparent_module`
(E1, E2) did the right thing, a one-line `use`. The result compiles and is accounted for by `verify`
("re-pointed through a module qualifier"), but the three call sites are long and rustfmt reflowed one.

## What would make it unnecessary

For a caller that already had a `use` of the old module, rewrite that `use` to the new module (what `reparent_module`
does) and keep the call's qualifier, instead of inlining the whole path.

## Minimal reproduction

```rust
// b.rs
use crate::old;                 // -> use crate::new;
fn g() { old::f() }             // -> new::f()      (today: crate::new::f(), with the use removed)
// plan: move_item old::f -> new
```

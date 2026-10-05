# 2026-10-05 — `restructure` `move_item` copies a moved item's text as written, so a facade path travels and undoes an earlier import clean-up

**Category:** Future enhancement (missing capability; closely related to the facade-import entry below)
**Source:** #carve 17/21 (PR #532) E1-E5

Related: [`2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate`](2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate.md)
(that one is about `use` items; this is a path written inline in the moved text).

## What happened

Acceptance check A4 (no `crate::<facade>` path in a T3 file) was empty after the hand clean-up `125f1899`. E3 moved
`split_forward_deadline(config: &crate::config::DaemonConfig)` from `svc_spawn_split_agent.rs` (a T4 file, where that
path was fine) into the T3 module `agent_roster.rs`. The engine copies the text by byte range, as designed, so
`agent_roster.rs:186` now holds `crate::config::DaemonConfig`, and A4 has one hit again. `crate::config` is
`pub use tddy_daemon_kernel::config`, a facade. Left as it is (a hand edit after an apply is a build correction
only); node 17 must fix it or the move into `tddy-session-agents` will present the shape
[`2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge`](2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md) refuses.

## What would make it unnecessary

An option on `move_item` (`canonical_paths: true`), or a note in its output, that rewrites a path in the moved text
whose head resolves through a `pub use` of another crate to the defining path (`goto_definition` on the head),
or lists such paths so the plan's author can see the regression before the commit. Combined with the facade-import
operation it would give A4 a mechanical answer.

## Minimal reproduction

```rust
// lib.rs
pub use other_crate::config;
// a.rs (allowed to use the facade)    pub fn f(c: &crate::config::Settings) {}
// plan: move_item a::f -> b           => b.rs:  pub fn f(c: &crate::config::Settings) {}   (still the facade)
```

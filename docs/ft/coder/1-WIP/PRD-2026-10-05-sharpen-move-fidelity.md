# `move_item` and `reparent_module` output fidelity - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — § Same-crate moves (how callers are re-pointed, what a facade is), § Moved code that changes meaning one module deeper, § Known limitations.
- **Related surface**: `.agents/skills/code-restructuring/references/plan-schema.md` — the plan schema (a new `canonical_paths` field on `move_item`) and the operation notes. Not a `docs/ft/` feature document, but the reference an author writes plans from. `SKILL.md` is deliberately not changed here.
- **Related Feature**: none other. `move_module_to_crate` and `move_cluster_to_crate` are unchanged; the path survey they use gains a second reader.

## Summary

`move_item` moves code correctly and a caller's result compiles, but three things it writes are worse than what a person would write, and each forced a hand edit during the lifecycle carve: a caller that reached the item through `use crate::old;` gets a long inline `crate::new::f()` path; a path in the moved code that goes through a `pub use` re-export of another crate travels unchanged; and a doc link to the moved item keeps pointing at the old path. This PRD makes the engine write the shorter caller form, optionally write the defining path in moved code, and re-point doc links.

## Background

Moving `tddy-session-lifecycle`'s items with `move_item` (the `#carve` 17/21 stages E1 to E5) produced results that compiled and that `restructure verify` accounted for, and that still needed follow-up: three call sites written as `crate::connection_service::agent_roster::split_forward_deadline(&self.config)` where `reparent_module` had written a one-line `use`; `agent_roster.rs:186` still holding `crate::config::DaemonConfig` where `crate::config` is `pub use tddy_daemon_kernel::config` (a path that blocks moving the module into another crate); and `handler_state.rs:106` linking an item path that no longer exists. The rules say a hand edit after an apply is a build correction only, so each was recorded as a todo instead.

## Proposed Changes

### What's Changing

- **Callers keep their qualifier shape (B1).** A caller that wrote the moved name behind a one-segment module qualifier bound by a `use` (`use crate::pairing;` then `pairing::f(x)`) reads `use crate::answers;` and `answers::f(x)` after `f` moves to `answers`. The old `use` is removed by the end-of-run unused-import tidy when nothing else uses it, and kept when something does. Callers that wrote the path in full are unchanged. Where the destination's name is already taken in the caller's scope the full path is written, as today, and the run's notes say so.
- **Moved code can name what it uses by its defining path (B2).** A new plan field, `canonical_paths: true` on a `move_item` line (default off; refused on every other operation), rewrites a `crate::`-headed path in the moved code that goes through a `pub use` of another crate to the path of the item where it is defined (`crate::config::Settings` becomes `kernel::config::Settings`). The run's notes list every path rewritten and every path left as written, with the reason (inside a grouped `use`, a defining module private to its crate, which no outside file may name, a span that does not read back as written).
- **Doc links follow the item (B3).** An intra-doc link in a `///` or `//!` line that names a moved item (or a re-parented module) by its old path is re-pointed to the new path, in the same run, for `move_item` and `reparent_module`. Prose and fenced examples are untouched. How the links are found depends on whether the language server reports them; see the changeset's probe.

### What's Staying the Same

- A plan with no new field behaves as before except for the two output changes above (shorter caller qualifiers, doc links following). Every existing plan still applies.
- `reexport` (`glob`, `named`, `outside`, `none`), the facade, the visibility widening, the compile gate, the tidy: unchanged.
- Splitting a grouped `use` that names several facade paths (and a standalone operation that rewrites a file's or module's imports through facades) is a separate, later change in this stack, not part of this one.
- `restructure verify`'s accounting rules.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: `backends/rust/item_move/sites.rs` (callers), a new `item_move/canonical_paths.rs`, possibly a new `item_move/doc_links.rs`, two `mod` declarations in `crate_move.rs` made `pub(crate)`, one `RefactorOp` field and a codec rule.
- The new field is a public struct field of `RefactorOp`: every full struct literal of it in the crate (20 in 15 files, mostly tests; measured with `git grep -n 'order: Vec::new()' -- packages/tddy-code-restructuring`) gains `canonical_paths: false`, in this node's first commit. No other package builds one.
- Two CI lists name the new live test binary.

### User Impact

- A plan author sees shorter, conventional call sites after a move and fewer hand fixes; a carve that must later move a module to another crate can ask for defining paths up front.
- The new field is opt-in. A caller whose `use` of the old module is still needed keeps it.
- A run stopped before its end (`--stop-after`) leaves an unused `use` as a warning, as it does today for copied imports; the run says so.

## Implementation Plan

1. Probe: does rust-analyzer return doc-link positions as references (decides B3's route).
2. B1 in `sites.rs`, red tests first.
3. Make the path survey reachable (`pub(crate)` modules), add the field and its codec rule, then B2.
4. B3 by the probe's route.
5. Register the new live test binary; update the plan schema and the feature doc.

## Acceptance Criteria

- [ ] A module-qualified caller keeps its qualifier and gains an import of the new module; a caller that already imports it gains none ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] An import still serving an unmoved item is kept; a full path the caller wrote is unchanged
- [ ] With `canonical_paths: true`, a facade path in the moved code is written as its defining path, and the notes list every path rewritten and every path left
- [ ] Without the field, the moved text is byte for byte what it was; the field is refused on any operation but `move_item`
- [ ] An intra-doc link to a moved item or re-parented module follows it (`cargo doc` resolves it); prose and fenced examples are untouched
- [ ] The existing same-crate move suites pass unchanged
- [ ] Tests passing for the packages touched (`./test -p tddy-code-restructuring`)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md) — Same-crate moves, Moved code that changes meaning one module deeper, Known limitations

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-05-sharpen-move-fidelity.md`
- Path survey: `packages/tddy-code-restructuring/docs/path-survey.md`
- Same-crate moves: `packages/tddy-code-restructuring/docs/same-crate-moves.md`

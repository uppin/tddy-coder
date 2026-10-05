# `repoint_facade_imports`: name a file's paths by the crate that defines them - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Rust operations (v1)` (a new operation), `## Path survey` (a third consumer of the survey), `## Verify` (what is accounted for).

No other feature document changes. The operation needs no new field, flag or wire message.

## Summary

A restructure plan can say "name everything in this file or module by the crate that defines it". `repoint_facade_imports`, anchored on a file or a module, rewrites each path that goes through a `pub use` of another crate inside the file's own crate
(`crate::config::DaemonConfig` where `pub use tddy_daemon_kernel::config;`) to the defining path, in `use` items at any depth, including grouped ones, and in bodies. `check --deep` lists the paths it would rewrite.

## Background

A module that will move to another crate must name what it uses by its defining crate, or the move shows an edge back to the crate it leaves. On `#carve` 17/21 twelve files and thirty lines were re-pointed by hand; four paths were in grouped
`use` items, which a text search misses, so the acceptance check had only a blind grep. The engine already resolves such a path for cross-crate moves; it had no operation that applies the answer by itself.

## Proposed Changes

### What's Changing

- New operation `repoint_facade_imports`; its line carries only an anchor (a `symbol` anchor names one file, an `items` anchor on a `mod` declaration names a module).
- Every path forwarded to a foreign crate is rewritten to its defining path. A group whose members agree has its prefix changed in place; a group whose members need different qualifiers is split: members that stay keep the group, each re-pointed member becomes its own `use`.
- Left alone: comments, doc comments and strings; paths to the crate's own items; in-crate facades; paths already written with a dependency's name.
- Refused, with the path, file and line, and nothing written: a defining crate the package does not depend on, a facade that renames an item used in a body, a path spelled across whitespace or comments, a rewrite that would bind a name twice, a group that must be split but carries an attribute or doc comment.
- Idempotent: running it again changes nothing and says so.
- `check --deep` (and `apply`) print one line per rewritten path: `file:line: written -> defined`. This also makes `check --deep` print the notes other operations already produce.

### What's Staying the Same

- Facades are never written, removed or edited, and no manifest is changed; moves still do that.
- The cross-crate moves, `move_item`, `reparent_module` and their behaviour; `verify` (it already accounts for re-pointed body paths and ignores `use` items).
- A path through *another* crate's re-export is not followed (a follow-up).

## Impact Analysis

### Technical Impact
- `tddy-code-restructuring` only: a new operation module, a codec rule file, wiring in `rust.rs`, visibility-only widening of the survey's modules, and a small change in the deep check.
- A thin live test binary joins the e2e filterset and the rust-analyzer test group; the main suite needs no rust-analyzer.

### User Impact
- A facade-import grep and hand edit become one plan line per module; `check --deep` answers "which paths would change" mechanically. No breaking change; the notes now visible in `check --deep` are the only output difference for existing plans.

## Implementation Plan

1. Plan surface and refusals. 2. Resolution and edits (library level, no server). 3. Group splitting. 4. Deep-check notes. 5. Thin live binary and its registration. 6. `verify` pins. 7. Docs at wrap.

## Acceptance Criteria

- [ ] a plain `use`, a grouped `use` (in place or split), a glob and a body path through a facade are re-pointed and the workspace compiles with its tests ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] comments, strings and own-crate paths are byte-identical afterwards
- [ ] every refusal above names the path and writes nothing; plain `check` reports them for a file-anchored plan
- [ ] a second run rewrites nothing
- [ ] `check --deep` prints each path it would rewrite and writes nothing
- [ ] `verify` holds for the result
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-05-sharpen-repoint-facade.md`
- Todo this closes (exists only on `feature/carve/lifecycle-ports-agents`, PR #532): `docs/dev/todo/2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate.md`
- Survey design: `packages/tddy-code-restructuring/docs/path-survey.md`

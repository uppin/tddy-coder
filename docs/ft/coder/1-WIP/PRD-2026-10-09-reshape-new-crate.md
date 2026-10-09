# Crate moves: create the destination crate, and carry every crate the moved code names - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement
**Status**: Approved 2026-10-09 (F1–F5 as recommended)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Rust operations (v1)` (the
  `move_module_to_crate` and `move_cluster_to_crate` rows gain `name`), `## Path survey` (what is surveyed; the
  manifest bullet), `## Known limitations`.

No other feature document changes. There is no new operation, CLI flag or wire message. The plan line reuses the
`name` field, which already means "the module this move creates" on `move_item`.

## Summary

A cross-crate move whose line carries `name` creates its destination crate in the same edit. The new crate gets a
`Cargo.toml` (`[package]` with that name and the origin's `version` and `edition`), a `src/lib.rs` that declares
the moved modules, and an entry in the workspace root's `members`. The destination's manifest also gains two kinds
of crate the engine misses today:

- a crate whose `use` imports an item with the crate's own name (`use async_trait::async_trait;`,
  `use anyhow::anyhow;`)
- a crate the origin declares only in a target-specific table (`[target.'cfg(unix)'.dependencies]`, e.g. `libc`).
  It lands in the same target table.

`check`, `check --deep` and `apply --dry-run` report a plan that creates a crate and then moves more into it the
same way `apply` performs it.

## Background

The `#carve` stack created seven crates. Each one needed a hand skeleton before the engine could move anything
into it, because a `to` without a `Cargo.toml` is refused (`is not a crate: …/Cargo.toml could not be read`). On
`#carve` 21/21 two crates were also missing dependency lines and had to be fixed by hand after the compile gate
failed:

- `tddy-demo-vm-service` lacked `async-trait`
- `tddy-cli-sessions` lacked `libc`, among others. The hand fix put it under `[dependencies]`, so it now builds on
  every target, where the origin had declared it for `unix` only.

The next stack splits `tddy-code-restructuring` itself into engine crates, and every one of those crates would
need the same hand skeleton. The skeleton is the only part of a crate move the engine cannot do today, and the
facts it needs are all in the repository already:

- the package name comes from the plan
- `version` and `edition` come from the crate being split
- the `members` entry is already written by today's moves

The two missed dependencies have plain causes:

- **`async-trait`**: the survey skips a path whose first segment the file binds itself. `use async_trait::async_trait;`
  binds `async_trait`, so its own crate head counts as bound and is dropped.
- **`libc`**: the manifest helpers read only `[dependencies]` and `[dev-dependencies]`, so a crate declared under
  `[target.'cfg(unix)'.dependencies]` is invisible. A body path such as `libc::kill` is not surveyed at all, and a
  `use libc::…` refuses the move as "declared nowhere".

## Proposed Changes

### What's Changing

- **`name` on `move_module_to_crate` and `move_cluster_to_crate`** is the `[package] name` of a crate the move
  creates at `to`. In the same edit as the move itself, the engine:
  - creates `<to>/Cargo.toml`: `[package]` with `name`, plus the origin's `version` and `edition` lines copied
    verbatim. Then the dependency tables the moved code needs, which the existing manifest pass fills.
  - creates `<to>/src/lib.rs`, which declares each moved module `pub mod <m>;` in sorted order. Nothing else is
    authored: no doc comment, no `description`, no `[lib]` table.
  - adds `"<to>"` to the workspace root's `members` array. This already happens for an existing destination that
    is missing from the list.

  A cluster creates its crate once. The rename, header rewrite, caller re-points, facade and dependent manifests
  are exactly what a move into an existing crate produces.
- **Refused, nothing written, the refusal naming the path:**
  - `name` when `<to>/Cargo.toml` already exists ("already a crate — drop `name` to move into it")
  - `name` when `<to>` already holds files
  - `name` that is not a valid Cargo package name
  - `name` whose extern name (`-` read as `_`) equals the origin's, or that of a crate the workspace already has
  - a root manifest with no `members` array, because cargo would not build the new crate. A move into an
    existing crate still skips that edit silently, as today
  - `name` on `move_test_binary_to_crate`. A test binary needs a crate that already exists to exercise.

  The refusal for a missing destination without `name` stays, and now says that `name` creates the crate.
  Today `name` on a crate move is silently ignored, so writing it was always a mistake. Refusing it on the test
  binary op closes that gap.
- **A plan can create a crate and then move more into it.** A later operation whose `to` is a crate an earlier
  operation of the same plan creates finds it:
  - in `apply`, from disk, as today
  - in `apply --dry-run` and `check --deep`, from the run's overlay. The destination manifest is read through the
    overlay instead of the disk.
  - in plain `check`, which treats the destination as created at that point in the plan, the way it already
    treats modules that earlier operations moved

  Creating the same crate twice in one plan is refused at the second operation.
- **The survey reads a crate imported under its own name.** A `use` whose first segment is a crate the origin
  declares counts as naming that crate, even when the same `use` binds that name in the file (`use x::x;`). A
  name the file binds in any other way still shadows the crate as before:
  - `mod x;`
  - an item named `x`
  - `use other::x;`
- **Target-specific dependency tables are read and written.** A crate the origin declares only under
  `[target.'<cfg>'.dependencies]`:
  - counts as declared for the body-path rule
  - is carried into the destination's `[target.'<cfg>'.dependencies]`, with the same cfg string verbatim. That
    table is created at the end of the manifest if missing.

  The same holds for `[target.'<cfg>'.dev-dependencies]` when only `#[cfg(test)]` code names the crate. A crate
  declared both plainly and under a target goes to the plain table. A crate under two different target tables is
  carried into both.
- `apply --dry-run`'s file count includes the two created files. No new note or output line is added, so
  `backends/rust.rs` keeps its current wiring.

### What's Staying the Same

- Every dependency line is copied verbatim from the manifest that declares it, with a relative `path`
  re-anchored. The engine never invents a version or a feature list. A `{ workspace = true }` line stays one.
  Nothing new goes into `[workspace.dependencies]`, so no second convention starts and the
  [repeated-versions debt](../../../dev/todo/2026-09-23-carved-crate-manifests-repeat-versions-and-tokio-feature-lists.md)
  is left for a decision about the whole workspace.
- The `members` entry is appended, as today, not sorted.
- Moves into an existing crate produce byte-identical edits except in two cases, both previously broken:
  - code that imports a crate under its own name
  - a crate declared only for a target
- The cycle refusal, the merge refusal, the facade rules and caller re-pointing are unchanged.
- `move_test_binary_to_crate` creates nothing, and its own scan of crate heads (`[dev-dependencies]`) is
  unchanged. Target tables for test binaries are deferred to a todo of their own.
- Directory children, and the crates only they name, belong to `#reshape` 5 (`feature/reshape/move-children`).
  Cross-crate widening belongs to `#reshape` 7.

## Impact Analysis

### Technical Impact

All changes are in `tddy-code-restructuring`:

- a new module that authors the skeleton
- `crate_move/destination.rs` reads through the overlay and accepts a planned creation
- `crate_move/manifest_edits.rs` learns target tables
- the survey's bound-name rule (`crate_move/survey.rs` and `test_binary.rs::names_bound_in`)
- a codec rule for `name`
- wiring in `crate_move/moving.rs` and `crate_move/cluster.rs`
- the static check's plan-order model in `crate_move/preconditions.rs`

Most tests run at library level with the existing fake reference set, without rust-analyzer. One live test joins
`tests/move_module_to_crate_acceptance.rs`, which is already registered in the e2e filterset and the
`rust-analyzer` group.

### User Impact

A plan can now say "move these modules into a new crate `tddy-x`" in one line. The skeleton, the `members` entry
and the manifest lines previously fixed by hand become engine output, which `check --deep` reports before
anything is written. This is not a breaking change: a plan that worked before produces the same edits.

## Implementation Plan

1. Plan surface: the `name` rules in the codec, and the refusals.
2. Skeleton authoring at library level: the manifest, `lib.rs` and members, for one module and for a cluster.
3. Reading the destination through the overlay, and plan-order creation in the static check.
4. The survey's same-name import rule.
5. Target-specific tables in the manifest helpers and the manifest pass.
6. One live test that creates a crate and compiles.
7. Docs at wrap.

## Acceptance Criteria

- [ ] `move_module_to_crate` with `name` into a directory with no crate creates `Cargo.toml`, `src/lib.rs` and
  the `members` entry, and the workspace compiles ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] `move_cluster_to_crate` with `name` creates the crate once, declaring every member
- [ ] the new manifest carries `version` and `edition` from the origin and only dependency lines copied from the
  manifests that declare them
- [ ] every refusal above names the path and writes nothing. Plain `check` reports them without an index
- [ ] a crate move without `name` into a missing crate is still refused, and the refusal mentions `name`
- [ ] a two-op plan, create then move into, passes `check`, `check --deep`, `apply --dry-run` and `apply` alike
- [ ] `use async_trait::async_trait;` and `use anyhow::anyhow;` in moved code add `async-trait` and `anyhow` to
  the destination. `mod x; use x::x;` adds nothing
- [ ] a body path `libc::kill(…)`, with `libc` declared under `[target.'cfg(unix)'.dependencies]`, lands in the
  destination's `[target.'cfg(unix)'.dependencies]`. `use libc::pid_t;` is no longer refused
- [ ] moves into existing crates with neither shape produce byte-identical edits (existing suites unchanged)
- [ ] tests pass for `tddy-code-restructuring` (scoped. CI covers the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) — `## Rust operations (v1)`, `## Path survey`,
  `## Known limitations`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-new-crate.md` (written after this PRD is reviewed)
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-new-crate-initial-discovery.md`
- Todo this closes: [the destination's manifest lacks `async-trait`](../../../dev/todo/2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro.md)
- Todo this partly closes (create-crate part. The children part belongs to `#reshape` 5):
  [also-members that are directory children, and their crates](../../../dev/todo/2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates.md)
- Todo this partly closes (acceptance item 3, the `libc` cause. Claimed by `#reshape` 5):
  [directory children left behind](../../../dev/todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md)
- Reference, left open: [carved crate manifests repeat versions](../../../dev/todo/2026-09-23-carved-crate-manifests-repeat-versions-and-tokio-feature-lists.md)
- Survey design: [path-survey.md](../../../../packages/tddy-code-restructuring/docs/path-survey.md)
- Plan schema: `.agents/skills/code-restructuring/references/plan-schema.md`

# Changeset: a crate move creates its destination and carries every crate the moved code names

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (crate moves gain crate creation; manifest pass bug fixes)
**Stack**: `#reshape` 9/19, branch `feature/reshape/new-crate`, [PR #606](https://github.com/uppin/tddy-coder/pull/606), wave 1. PR title:
`feat(code-restructuring): a crate move creates its destination and carries every crate it names (#reshape 9/19)`.
Base in the linear stack: `feature/reshape/move-grouped-use` (K=8). **Real edges**: none. Nothing this node consumes comes from another
node, and no node consumes this one. It sits on the line only because `gh stack` needs a line. With nodes 5, 7 and 8 it shares
`crate_move/cluster.rs`, `crate_move/moving.rs` and `crate_move/manifest_edits.rs`, so they collide textually, not behaviourally.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-new-crate-initial-discovery.md)
(Exploration 1: whole-work backlog discovery; Exploration 2: this node).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds nothing claimed. **No 🚧 claimed issue is
in the path, and there is no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro.md](../todo/2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro.md) | ✅ **RESOLVED HERE** | The real cause is narrower than "attribute macro". `use async_trait::async_trait;` binds its own crate head, so the survey drops it (`survey.rs:84`). Fixed by rule S1 below (tests 16, 19, 20). Deleted at wrap |
| [2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates.md](../todo/2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates.md) | ⚠ **partial** (claimed by `#reshape` 5) | Its "offer a create-crate step so a destination need not exist" sentence and the hand-skeleton bullet: rules C1–C6 (tests 1–15, 26). The children part and the manifest of the children belong to `feature/reshape/move-children`. This node's wrap removes the create-crate sentence and bullet. The file stays for node 5 |
| [2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md](../todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md) | ⚠ **partial** (claimed by `#reshape` 5; slice reassigned here by the developer 2026-10-09) | Acceptance item 3, the `libc` cause. The origin declares `libc` only under `[target.'cfg(unix)'.dependencies]`, which `manifest_edits::dependencies_of` never reads. Fixed by rule T1 (tests 18, 21–25). This node's wrap narrows item 3 to "children only" |
| [2026-09-23-carved-crate-manifests-repeat-versions-and-tokio-feature-lists.md](../todo/2026-09-23-carved-crate-manifests-repeat-versions-and-tokio-feature-lists.md) | — Reference, left open | New manifests copy lines verbatim and start no `[workspace.dependencies]` convention, so they add nothing new to that decision. They do repeat versions, exactly as hand-carved crates do |
| [../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md) | — Unrelated (deliberately untouched) | Rule S1 lives in `survey.rs`, not in `test_binary.rs::names_bound_in`. The scanner `#reshape` 15 extracts is not edited |
| Function-size list (whole-work discovery, Exploration 3): `plan/codec.rs::parse_op` (206), `crate_move/cluster.rs::resolve_cluster` (66), `crate_move/cluster/stranded.rs::stranded_siblings` (70) | ⚠ **DURING** (owned by `#reshape` 16) | None of them grows. The codec rule is called from `parse_ops`, not `parse_op`. `#reshape` 7 already moved `resolve_cluster`'s body into the private `cluster_edits`; creation hooks into `widened_cluster`, its 3-line caller, so neither function grows (node 16's list entry is now `cluster_edits`). The `stranded.rs` edit is one changed line in `modules_the_plan_moves` |
| `docs/dev/todo/2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md` | — Already closed by #540 | The 10-08 entry links to it, but it no longer exists. The body-path survey it asked for is what rule T1 extends |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md)
  - new `src/crate_move/new_crate.rs`, and new test files `src/crate_move/new_crate_tests.rs` and
    `src/crate_move/manifest_pass_tests.rs`
  - `src/crate_move.rs`: `mod new_crate;` and two `#[cfg(test)] mod …_tests;` lines; `resolve` passes the creation through
  - `src/crate_move/destination.rs`: `read_in` and the refusal wording
  - `src/crate_move/moving.rs`: `Move.creates`, `CarriedLines`, target tables in `destination_manifest`
  - `src/crate_move/cluster.rs`: `MovingCluster.creates`, `resolve_cluster` becomes a wrapper, `named_by`
  - `src/crate_move/cluster/stranded.rs`: one line, `read` becomes `read_in`
  - `src/crate_move/manifest_edits.rs`: target tables, `with_lines_under`, `package_line`
  - `src/crate_move/survey.rs`: rule S1, plus a `declares_dependency_in_any_table` call
  - `src/crate_move/preconditions.rs`: creation refusals, earlier creations seeded into the static check
  - new `src/plan/codec/crate_move_fields.rs`, and `src/plan/codec.rs` (one `mod`, one call in `parse_ops`)
  - `src/lib.rs`: re-export `NewCrate`
  - `tests/move_module_to_crate_acceptance.rs` (one live test) and new `tests/new_crate_acceptance.rs`
  - Docs at wrap:
    - [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md): "What is surveyed", "Rewrite, edges, manifest", "Limits"
    - a "New crates" section in the package README (or a new `docs/crate-creation.md` if it exceeds a screen)
    - [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)
    - [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md): the `name` column for the crate-move rows,
      and the refusal list
- **`tddy-tools`, `tddy-index-daemon`, `tddy-daemon-rpc`**: no source change.
- **`.config/`**: no change. The live test joins `move_module_to_crate_acceptance`, which is already registered in both files.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-new-crate.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-new-crate.md) (this PRD; approved 2026-10-09)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Rust operations (v1)`, `## Path survey`, `## Known limitations`

## Summary

`move_module_to_crate` and `move_cluster_to_crate` accept `name`: the `[package] name` of a crate the move creates at `to`. The new
crate's `Cargo.toml` (origin's `version` and `edition`, verbatim), `src/lib.rs` and root `members` entry are part of the move's single
edit. A plan may create a crate and move more into it later, and `check`, `check --deep`, `--dry-run` and `apply` agree on that plan.
The manifest pass also stops missing two kinds of crate:

- one whose `use` imports an item with the crate's own name (`async-trait`, `anyhow`)
- one the origin declares only in a target-specific table (`libc`). It is carried into the same target table.

## Background

Every crate `#carve` created needed a hand skeleton, because `Destination::read` refuses a `to` without a manifest
(`destination.rs:28-35`, a rule its doc comment states and `crate_move.rs:600-617` pins). On `#carve` 21/21:

- `tddy-demo-vm-service` lacked `async-trait`
- `tddy-cli-sessions` lacked `libc`. The hand fix flattened it into `[dependencies]`, losing `cfg(unix)`.

The second stack (the `tddy-code-restructuring` crate split) creates several engine crates. With this node it can create them with
engine moves only.

## Responsibility

- **Crate creation** (rules C1–C6): the `name` field on the two module crate moves, the skeleton, the `members` entry, the refusals,
  and one creation per cluster.
- **Plan-order parity** (rule P1): the destination is read through the overlay, and the static check seeds the crates earlier
  operations create.
- **Self-named imports** (rule S1) in the path survey.
- **Target-specific dependency tables** (rule T1) in the manifest helpers and the module/cluster manifest pass.
- One live test proving that a created crate compiles.

## Plan-line schema and the rules (the contract)

```jsonl
{"op":"move_cluster_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-code-restructuring/src/plan.rs","path":"plan"},"also":[{"kind":"symbol","file":"packages/tddy-code-restructuring/src/edit.rs","path":"edit"}],"to":"packages/tddy-restructure-plan","name":"tddy-restructure-plan","reexport":"glob"}
{"op":"move_module_to_crate","anchor":{"kind":"symbol","file":"packages/tddy-code-restructuring/src/apply.rs","path":"apply"},"to":"packages/tddy-restructure-plan","reexport":"glob"}
```

The first line creates `packages/tddy-restructure-plan`. The second moves into the crate the first created and carries no `name`.

**C1. `name`.** On `move_module_to_crate` and `move_cluster_to_crate`, `name` is the new crate's `[package] name`. The codec refuses it in
these cases (`plan/codec/crate_move_fields.rs`, called from `parse_ops`, so it sees the whole plan):

- on `move_test_binary_to_crate` ("a test binary moves into a crate that exists — `name` creates one only for a module move")
- when it is not a Cargo package name. A non-empty run of ASCII letters, digits, `-` and `_` is accepted. A leading digit is refused,
  and so is the reserved name `test`.
- when two operations of one plan carry `name` with the same `to` ("`<to>` is created by operation N already")

Everywhere else `name` keeps its present meaning (`move_item`) or its present refusal (`reparent_module`, the signature ops).

**C2. Refusals that need the tree** (`NewCrate::refusal`, static, so plain `check` reports them; nothing is written, and the refusal names
the path):

- `<to>/Cargo.toml` exists: "`<to>` is already a crate — drop `name` to move into it"
- `<to>` exists and holds any entry: "`<to>` is not empty"
- the extern name (`-` read as `_`) equals the origin's, or that of a crate the root `members` array lists explicitly (glob entries are
  not expanded): "`<extern>` is already the extern name of `<dir>`"
- the root `Cargo.toml` has no `members` array: "the workspace lists no `members`, so cargo would not build `<name>`"

A move **without** `name` into a missing crate keeps today's refusal and gains the remedy: "`<to>` is not a crate:
<to>/Cargo.toml could not be read — add `name` to create it".

**C3. The skeleton** (`NewCrate::skeleton`). Two `FileEdit::Create` entries (`<to>/Cargo.toml`, `<to>/src/lib.rs`) and the manifest text:

```toml
[package]
name = "<name>"
<origin's version line, verbatim>
<origin's edition line, verbatim>
```

The version and edition lines are copied as written, so `version.workspace = true` stays that way. If the origin has no `version` or
`edition` line, that line is omitted; it is never invented. `src/lib.rs` starts empty. Nothing else is authored: no doc comment, no
`description`, no `[lib]`, no `publish`.

**C4. What fills it.** The move resolves exactly as it does into an existing crate, against a workspace whose overlay already holds the
skeleton:

- `declared_in_destination` inserts each `pub mod <m>;` sorted
- `destination_manifest` appends `[dependencies]`, `[dev-dependencies]` and target tables as needed
- `workspace_members` appends `"<to>",`

**C5. Folding.** A created file's edits are folded into its skeleton text (`crate::apply::edited`), and the file is emitted as one
`Create` plus one `Change` that inserts the final text at 1:1. No path gets two `Change`s, so `PositionLedger`, `Overlay::record` and
the journal see one edit per file, as they do today.

**C6. One creation per cluster.** `MovingCluster.creates` is set by `cluster::named_by` (from the op) and by `travelling_alone` (from
`Move.creates`). The skeleton is built once for the set, before any member resolves.

**P1. Plan order.**

- `Destination::read_in(workspace, dir)` reads the manifest through `Workspace::read`, that is, through the overlay. The destination
  sites use it: `moving.rs:53`, `cluster.rs:88`, `cluster/stranded.rs:191-192`. `test_binary.rs:59` stays on `read`, since this node
  leaves the test-binary op alone.
- `check --deep` and `--dry-run` already fold each operation into their overlay (`rehearsal.rs:64-65`, `store_run.rs:331-332`), so a
  later operation finds the earlier one's crate.
- Plain `check` (`preconditions::unrunnable`) builds, for operation N, an overlay holding the skeletons of every earlier operation's
  creation (`new_crate::created_by_earlier_operations`) and checks N against it. A second `name` on the same `to` is already refused by
  C1, and by C2 under `--deep`.

**S1. A crate imported under its own name.** In `survey.rs`, `names_bound_by(text, manifest)` drops from the bound set every name `x`
that meets both conditions:

- every `use` binding `x` is a leaf whose first segment is `x`, with at least two segments and no alias (`use x::x;`,
  `use x::{x, Y};`), and nothing else binds `x` (no `mod x`, no item `x`, no `use other::x`)
- the origin's manifest declares `x` in any table (T1)

The head `x` is then surveyed as the crate it is, in `use` items and bodies alike.

**T1. Target-specific tables.**

- `manifest_edits` reads `[target.'<cfg>'.dependencies]` and `[target.'<cfg>'.dev-dependencies]`, with the header matched verbatim,
  including the quotes.
- For a body path, the survey asks `declares_dependency_in_any_table`, so a crate declared only under a target counts.
- `Move::dependency_lines` decides per named crate:
  - When the origin declares it in the plain table being filled, that table wins (unchanged).
  - Otherwise, each target table of the matching kind that declares it (`dependencies` for code, `dev-dependencies` for
    `#[cfg(test)]`-only code, falling back to the target `dependencies` table as the plain dev pass does) contributes its line,
    re-anchored, under the same header in the destination.
  - A crate in no table at all is refused as today.
- A destination that already declares the crate under that header, or in its plain table, gains nothing.
- `with_lines_under(manifest, header, lines)` appends the lines at the end of that table, or appends the table at the end of the
  manifest.

## Boundaries

- **No change to `move_test_binary_to_crate`**: no creation, and no target tables in its own scan (deferred to a todo).
- **No version, feature list or `[workspace.dependencies]` entry is authored.** Lines are copied verbatim, a relative path is
  re-anchored, and the path back to the origin is authored as today.
- **Directory children** (node 5), **widening** (node 7), **grouped `use` and `pub(in …)`** (node 8) and **destinations in the pre-apply
  gate** (node 10) are not touched. A crate created here has no pre-apply package, so node 10 has nothing to add for it.
- **No new output line or note.** `backends/rust.rs` (node 17's file) is not edited.
- **No growth of a function on the >60 list** (`parse_op`, `resolve_cluster`, `stranded_siblings`, `sightings`, `items_of_module`). New
  logic goes in new functions.
- `test_binary.rs` is not edited.
- The `members` entry is appended, not sorted (unchanged behaviour).

## Dependencies

This node has no parent: no behaviour of another `#reshape` node is consumed. It is greenable on `master` alone.

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR's contract, not its deliverable). **Owned surface, new today**:

- `pub struct NewCrate { pub dir: String, pub package: String }` (`crate_move/new_crate.rs`, re-exported from `lib.rs`), with:
  - `pub(crate) fn named_by(op: &RefactorOp) -> Option<NewCrate>`
  - `pub(crate) fn destination(&self) -> Destination`
  - `pub(crate) fn refusal(&self, workspace: &Workspace<'_>, origin: &Destination) -> Result<Option<String>>`
  - `pub(crate) fn skeleton(&self, workspace: &Workspace<'_>, origin: &Destination) -> Result<Skeleton>`
- Free functions in `new_crate`:
  - `pub(crate) fn folded(skeleton: &Skeleton, changes: Vec<FileEdit>) -> Result<Vec<FileEdit>>`
  - `pub(crate) fn created_by_earlier_operations(workspace: &Workspace<'_>, ops: &[RefactorOp], index: usize) -> Result<Overlay>`
  - `pub(crate) struct Skeleton { manifest: String, root: String, created: [String; 2] }`
- `MovingCluster.creates: Option<NewCrate>` (pub field) and `moving::Move.creates: Option<NewCrate>`.
- `Destination::read_in(workspace: &Workspace<'_>, dir: &str) -> Result<Destination>` (pub).
- ~~`cluster::resolve_members_together`~~ — dropped at the contract (see Validation results): `#reshape` 7 already moved the body
  into private `cluster_edits`, so creation hooks into `cluster::widened_cluster` and no rename is needed.
  `resolve_cluster` keeps its public signature.
- In `manifest_edits`:
  - `pub(crate) struct DeclaredIn { pub(crate) header: String, pub(crate) line: String }`
  - `pub(crate) fn target_declarations(manifest: &str, extern_name: &str, dev: bool) -> Vec<DeclaredIn>`
  - `pub(crate) fn declares_dependency_in_any_table(manifest: &str, extern_name: &str) -> bool`
  - `pub(crate) fn with_lines_under(manifest: &str, header: &str, lines: &[String]) -> Vec<TextEdit>`
  - `pub(crate) fn package_line<'a>(manifest: &'a str, key: &str) -> Option<&'a str>`
- `moving::CarriedLines { plain: Vec<String>, by_target: BTreeMap<String, Vec<String>> }`, returned by `Move::dependency_lines`.
- In `survey`: `fn names_bound_by(text: &str, manifest: &str) -> BTreeSet<String>` and `fn bound_only_by_their_own_crate(text: &str) -> BTreeSet<String>`.
- `plan::codec::crate_move_fields::refuse_crate_creations_it_cannot_honour(ops: &[RefactorOp]) -> Result<()>`.
- Failing tests: the 24 red ones in "Acceptance tests" (17 and 23 are green guards).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes, on `master` alone.
**Concurrent with:** every other wave-1 node (`widen-same-crate`, `multi-seam-extract`, `tidy-facades`, `extract-method-clean`,
`move-children`, `methods-leave-type`, `move-widen`, `move-grouped-use`, `apply-robust`, `move-item-paths`, `anchors-outline`).
**Blocks:** none.
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`, `4→19`, `17→19`. None
involves node 9. Expected textual conflicts are with `move-children` (`cluster.rs`, `moving.rs`), `move-widen` and `move-grouped-use`
(`moving.rs`, `manifest_edits.rs`), and `fn-sizes-rest` (the `resolve_cluster` rename).

## Successor PRs

None depend on this node. The second stack (the crate split) consumes `name`; it is planned after `#reshape` lands.

## Scope

- [ ] **Plan surface**: `crate_move_fields.rs`, C1 refusals, `NewCrate` re-export
- [ ] **Creation**: `new_crate.rs` (C2–C6), `Move.creates`, `MovingCluster.creates`, the `resolve_cluster` wrapper
- [ ] **Plan order**: `Destination::read_in` at the destination sites; seeded static check
- [ ] **Survey**: rule S1
- [ ] **Manifest**: rule T1 in `manifest_edits` and `Move::dependency_lines`
- [ ] **Live test** in `move_module_to_crate_acceptance.rs`
- [ ] **Package documentation** at wrap (list under Affected Packages); narrow the 10-04 and 10-08 also-members entries, delete the
  10-08 async-trait entry
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`,
  `cargo fmt`; no file past 500 production lines; no function on the >60 list grows

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `Destination::read` (`destination.rs:28-35`) reads `<to>/Cargo.toml` from disk and refuses a missing one; it bypasses the overlay
  that every other read uses (`registry.rs:32-34`).
- `name` on a crate move parses and is ignored (`plan/codec.rs` has no rule for it).
- The static check checks each operation against the original tree (`preconditions.rs:49-66`).
- `workspace_members` (`moving.rs:250-273`) already appends a missing destination to the root `members`. `declared_in_destination`
  (`moving/facade_writer.rs:291-314`) and `destination_manifest` (`moving.rs:117-149`) work on any manifest and root text.
  `FileEdit::Create` with `git add -N` already exists (`apply.rs:167-176`).
- `survey.rs:84` skips a head the file binds, and `name_bound_by` (`test_binary.rs:458-469`) counts `use x::x;` as binding `x`.
- `manifest_edits::dependencies_of` (`:141-150`) reads `[dependencies]` and `[dev-dependencies]` only. As a result:
  - `survey.rs:87-90` ignores `libc::…` body paths
  - `moving.rs:184-196` refuses `use libc::…`

### State B (Target)

A plan line with `name` creates a buildable crate with exactly what the moved code needs. The four entry points agree on plans that
create and then fill a crate. `async-trait`, `anyhow` and `libc`-style crates arrive in the destination's manifest under the table their
origin declares them in.

### Delta (What's Changing)

#### `tddy-code-restructuring`

- **New** `crate_move/new_crate.rs` (~180 production lines): `NewCrate`, `Skeleton`, `refusal`, `skeleton`, `folded`,
  `created_by_earlier_operations`, the package-name check shared with the codec.
- **`crate_move/cluster.rs`**: `MovingCluster.creates`; `named_by` sets it; `travelling_alone` copies it. `widened_cluster` (which
  `resolve_cluster` calls) branches: no creation, as today. With a creation: refusal, then skeleton, then a seeded overlay clone, then
  `cluster_edits` and the widening against it, then `folded`. `cluster_edits` is not edited.
- **`crate_move/moving.rs`**:
  - `Move.creates`; `Move::read` reads `NewCrate::named_by(op)` and uses `NewCrate::destination()` when it is set, otherwise
    `Destination::read_in`
  - `dependency_lines` returns `CarriedLines`
  - `destination_manifest` emits one `with_lines_under` per target header, through a new helper `target_table_edits` so that
    `destination_manifest` stays under 40 lines
- **`crate_move/destination.rs`**: `read_in`, plus the "add `name` to create it" remedy in the refusal (shared by `read` and `read_in`).
- **`crate_move/manifest_edits.rs`**: `DeclaredIn`, `target_declarations`, `declares_dependency_in_any_table`, `with_lines_under`
  (`with_dependencies` delegates to it), `package_line`.
- **`crate_move/survey.rs`**: `names_bound_by(text, manifest)` and `bound_only_by_their_own_crate`; line 88 uses
  `declares_dependency_in_any_table`. `survey_moved_file` keeps its line count.
- **`crate_move/preconditions.rs`**:
  - `unrunnable` checks each operation against `created_by_earlier_operations`
  - `move_preconditions` runs `NewCrate::refusal` when the op creates, and then skips `destination_already_has_the_module`, since a
    new crate cannot already hold the module
- **`crate_move/cluster/stranded.rs`**: `Destination::read(workspace.root, …)` becomes `Destination::read_in(workspace, …)`.
- **`plan/codec.rs`** (`mod crate_move_fields;`, one call in `parse_ops`) and **new** `plan/codec/crate_move_fields.rs` (~60 lines).
- **`lib.rs`**: `NewCrate` in the `crate_move` re-export.

## Implementation milestones

- [ ] **M1** plan surface (C1); tests 1–3
- [ ] **M2** `Destination::read_in` and the refusal remedy; test 9
- [ ] **M3** skeleton, folding, the cluster wrapper (C3–C6); tests 10–15
- [ ] **M4** tree refusals and the seeded static check (C2, P1); tests 4–8
- [ ] **M5** rule S1; tests 16–17, 19–20
- [ ] **M6** rule T1; tests 18, 21–25
- [ ] **M7** live test; test 26
- [ ] **M8** docs staged, scoped gate, length gates

## Testing plan

### Testing Strategy

**Primary: library level, no rust-analyzer.**

- Creation and the manifest pass are decided from text. The in-crate tests use the `crate_move` fixture style: a tempdir workspace,
  an `Overlay`, and the fake `ModuleReferences` reference set. `ModuleReferences` is not public, so these tests live inside the crate as
  `*_tests.rs` modules, as `runner/tidy/wide_facade_tests.rs` does.
- The plan codec and the static check are reached through the public API (`Plan::parse`, `unrunnable_moves`) from a new integration
  test binary.
- **One live test** proves that a created crate is built by `cargo check` and that rust-analyzer's survey of the origin works when the
  destination does not exist yet.

#### Option 1 (chosen): in-crate fixtures plus one public-API binary

**Trade-off**: exact on text and fast. A registry crate (`async-trait`, `libc`) is never compiled. The fixtures assert the copied lines;
the live fixture keeps to path crates, as every existing live fixture does.

#### Option 2 (rejected): live tests for `async-trait` and `libc`

These would make the live suite fetch registry crates for the first time. Tests 19–25 already pin the manifest text, which is the whole
defect.

### Coverage Requirements

- [ ] Happy: module and cluster creation; create then fill; self-named imports; target tables (code, test-only, existing table)
- [ ] Refusals: every C1/C2 case, and the remedy text for a missing crate without `name`
- [ ] Parity: plain `check`, `check --deep` overlay and `apply` agree on a create-then-fill plan
- [ ] Untouched: moves into existing crates with neither shape (the existing suites, unchanged); shadowing by `mod`/item/other `use`
- [ ] Actual effects: bytes on disk and `cargo check` (test 26)

The existing `crate_move.rs:600-617` `refuses_a_destination_with_no_manifest` stays green unchanged (its `naming` checks still hold).

## Acceptance tests

24 of the 26 are **red on `master`**; tests 17 and 23 are guards that are green today. Each red test's reason is given in its group heading or after its name.

### `packages/tddy-code-restructuring/tests/new_crate_acceptance.rs` (new; public API, no server)

Plan codec. *Red today*: `name` on a crate move is silently accepted.

1. `name_on_a_test_binary_move_is_refused_naming_the_field`
2. `a_name_that_is_not_a_cargo_package_name_is_refused_naming_it` (`tddy x`, `1x`, `test`, empty)
3. `a_plan_that_creates_the_same_crate_twice_is_refused_naming_the_earlier_operation`

Static check via `unrunnable_moves`. *Red today*: tests 4–8 report "is not a crate" or nothing, and test 9's refusal lacks the remedy.

4. `a_static_check_of_a_plan_that_creates_a_crate_and_then_moves_into_it_reports_nothing`
5. `a_static_check_reports_name_on_a_destination_that_is_already_a_crate`
6. `a_static_check_reports_name_on_a_directory_that_is_not_empty`
7. `a_static_check_reports_a_name_whose_extern_name_the_origin_or_a_listed_member_already_has`
8. `a_static_check_reports_a_crate_creation_in_a_workspace_with_no_members_list`
9. `a_move_into_a_missing_crate_without_name_is_refused_and_says_name_creates_it`

### `packages/tddy-code-restructuring/src/crate_move/new_crate_tests.rs` (new; in-crate, fake reference set)

*Red today*: the move is refused as "not a crate".

10. `a_module_moved_with_name_creates_its_crate_with_a_manifest_and_a_root_declaring_it`. The exact `Cargo.toml` is
    `[package]` + name + `version = "0.1.0"` + `edition = "2021"` + `[dependencies]` with the carried path dependency re-anchored. The
    root is `pub mod host_registry;\n`. Both are a `Create` plus one `Change` each.
11. `the_new_manifest_copies_the_origins_version_and_edition_lines_as_written` (origin `version.workspace = true`, `edition = "2024"`)
12. `a_crate_created_by_a_move_is_added_to_the_workspace_members`
13. `a_cluster_moved_with_name_creates_its_crate_once_and_declares_every_member_in_sorted_order`
14. `a_move_into_a_crate_an_earlier_operation_created_resolves_through_the_overlay`. Op 2 is resolved against an `Overlay` that recorded
    op 1's edit, and nothing is on disk.
15. `a_created_crate_depends_on_the_origin_by_a_path_relative_to_its_own_directory` (the edge back to a module that stays)

### `packages/tddy-code-restructuring/src/crate_move/survey.rs` (existing `mod tests`)

16. `a_crate_imported_under_its_own_name_is_surveyed_as_that_crate`: `use async_trait::async_trait;` has `defining_crate` `async_trait`.
    *Red today*: dropped as bound.
17. `a_name_bound_by_a_module_an_item_or_another_import_still_shadows_the_crate`: `mod x; use x::x;` and `use other::x;` with a body
    `x::y` give no `x` crate path. A guard, green today; it pins that S1 is narrow.
18. `a_body_path_to_a_crate_the_origin_declares_only_for_a_target_is_surveyed` (`libc::kill`, `[target.'cfg(unix)'.dependencies]`).
    *Red today*.


### `packages/tddy-code-restructuring/src/crate_move/manifest_pass_tests.rs` (new; in-crate, fake reference set, existing destination)

*Red today*: tests 19–20 add nothing; tests 21, 23, 24 and 25 add nothing or the wrong table; test 22 is refused.

19. `a_moved_file_importing_async_trait_adds_async_trait_to_the_destination`, with the line copied verbatim (`async-trait = "0.1"`)
20. `a_moved_file_importing_anyhow_by_its_own_name_adds_anyhow`
21. `a_crate_the_origin_declares_only_for_unix_lands_in_the_destinations_unix_table`
22. `a_use_of_a_crate_declared_only_for_a_target_is_carried_rather_than_refused` (`use libc::pid_t;`)
23. `a_crate_declared_both_plainly_and_for_a_target_goes_to_the_plain_table_only`
24. `a_crate_only_test_code_names_lands_in_the_target_dev_dependencies_table`
25. `a_target_table_the_destination_already_has_gains_the_line_under_it_not_a_second_header`

### `packages/tddy-code-restructuring/tests/move_module_to_crate_acceptance.rs` (existing live binary; filterset and `rust-analyzer` group already registered)

26. `relocates_a_module_into_a_crate_the_move_creates_and_leaves_every_crate_compiling`. The fixture is
    `a_workspace_a_module_can_move_across`, with `to: crates/fresh` and `name: fresh`. The test asserts:
    - the manifest and root exist
    - the root `members` lists `crates/fresh`
    - the caller is re-pointed to `fresh::host_registry::HostRegistry`
    - `assert_compiles`

    *Red today*: refused as "not a crate".

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (2026-10-09, wave-1 PRD review): "All 12 wave-1 PRDs approved. Every node's own F-decisions: take the agent's
recommendation." Reassignment: "`libc` / `[target.'cfg(..)'.dependencies]` tables → node 9 (partial of node 5's claimed 10-04 entry,
item 3)."

**Decided** (recommendations taken):

- **F1 — creation is opt-in through `name`.** Taken over implicit creation for a missing `to`: it guards against typos, reuses
  `move_item`'s meaning of `name`, and keeps the existing refusal's rationale.
- **F2 — the skeleton** carries the origin's `version` and `edition` lines verbatim and nothing else (no `description`).
- **F3 — target-table crates go to the same `cfg` table**, not flattened into `[dependencies]` as the `#carve` hand fix did.
- **F4 — no `members` array refuses a creation.** A move into an existing crate keeps skipping that edit silently.
- **F5 — test-binary target tables are deferred** to a todo (`2026-10-09-restructure-test-binary-move-reads-no-target-specific-dependency-table.md`).

Decisions taken by this plan:

- No output line or note, so `backends/rust.rs` stays untouched.
- Folding (C5) over sequential `Change`s for created files: one edit per path keeps ledger and journal assumptions intact.
- The `resolve_cluster` body is renamed rather than grown.
- The codec rule sits in `parse_ops` rather than `parse_op`.
- Glob `members` entries are not expanded for the clash check (C2). A clash through a glob member surfaces at the compile gate
  (`cargo` refuses two packages with one name).

**OPEN**: none.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

### Draft PR contract (commit 2, 2026-10-09)

Scoped gate on the contract commit: `cargo check -p tddy-code-restructuring --all-targets`,
`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.

**Surface published.** As listed under `## Draft PR contract`. Unimplemented bodies are `todo!()` marked
`TODO(reshape-new-crate)`, and nothing is wired yet, so no existing path reaches a `todo!()`. Uncalled items carry
`#[allow(dead_code, reason = "TODO(reshape-new-crate)…")]`; green removes each one as it wires it. Differences from the
planned contract:
- `NewCrate::destination` is implemented already, because it only builds the `Destination`.
- `resolve_members_together` is dropped, as described above.
- `MovingCluster.creates` and `Move.creates` exist and are always `None`.
- `names_bound_by` already takes `manifest` but ignores it for now.
- The codec function is not yet called from `parse_ops`, since a `todo!()` there would break every plan.

**Required-field touch in parents' tests.** `creates: None` was added to the `MovingCluster` literals in
`tests/cluster_move.rs`, `tests/crate_move_children.rs`, `tests/crate_move_restricted_declarations.rs`,
`tests/grouped_use_crate_move.rs`, `tests/move_to_crate_widening.rs` and `src/crate_move/cluster.rs`'s test module.

**Acceptance tests.** Run per binary or filter, never the whole suite. 24 are red because the implementation is missing;
2 are guards that are green today.
- 1–3 (`tests/new_crate_acceptance.rs`): red. `Plan::parse` accepts the line.
- 4–9 (`tests/new_crate_acceptance.rs`): red. The static check reports "is not a crate" without the remedy, or reports
  nothing (test 5).
- 10–15 (`src/crate_move/new_crate_tests.rs`): red. The move is refused as "`crates/fresh` is not a crate".
- 16 and 18 (`src/crate_move/survey.rs`): red. The survey returns `[]`.
- **17: guard, green**, as planned.
- 19–21, 24, 25 (`src/crate_move/manifest_pass_tests.rs`): red. The destination's manifest gains nothing.
- 22: red. The move is refused with "names `libc`, which … does not declare".
- **23: guard, green.** The plain table already wins today. The changeset had counted it as red; it pins that the
  target-table pass leaves this case alone.
- 26 (`tests/move_module_to_crate_acceptance.rs`, live): red. The resolution is refused as "not a crate".

**Surface unit tests**: 5 tests, all red with `not yet implemented`.
- 4 in `crate_move/manifest_edits.rs` `mod target_table_tests`, covering `target_declarations`,
  `declares_dependency_in_any_table`, `with_lines_under` and `package_line`.
- 1 in `crate_move/survey.rs` (`a_name_bound_only_by_its_own_crates_import_is_listed_and_one_bound_otherwise_is_not`),
  covering `bound_only_by_their_own_crate`.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-new-crate-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-new-crate.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review. It also:
  - deletes `2026-10-09-reshape-new-crate-initial-discovery.md` and the 10-08 async-trait todo
  - narrows the 10-08 also-members todo (drops create-crate) and item 3 of the 10-04 todo (drops `libc`)
- [ ] USER REVIEW — work complete, decide next steps

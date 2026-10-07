# Changeset: `repoint_facade_imports` names what a file uses by the crate that defines it

**Date**: 2026-10-05
**Status**: 🚧 In Progress
**Type**: Feature (new restructure operation; text edits and manifests read only)
**Stack**: `#sharpen` 8/8, draft PR [#595](https://github.com/uppin/tddy-coder/pull/595), branch `feature/sharpen/repoint-facade`, wave 2. PR title:
`feat(code-restructuring): repoint_facade_imports names paths by the crate that defines them (#sharpen 8/8)`.
Base in the linear stack: `feature/sharpen/repoint-call` (K=7). **Real edges**: `tidy-engine-files` (K=1, file overlap and the new home of `RefactorKind`) and `move-fidelity` (K=2, the resolver's `pub(crate)` module visibility).
`retarget-impl` (K=6) would become an edge only if decision F3 is taken as "copy attributes" (the `verify` carrier); it is not one under the recommendation.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-05-sharpen-repoint-facade-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `tddy-code-restructuring`, `tddy-tools` and `tddy-index-daemon` finds one file whose value is `none` (#537 merged): **no 🚧 claimed issue is in the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| `docs/dev/todo/2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate.md` — **exists only on `feature/carve/lifecycle-ports-agents` (PR #532)**; whichever of that PR and this node lands second deletes it at wrap. Its stale link to a todo that #540 deleted is replaced by `packages/tddy-code-restructuring/docs/path-survey.md` | ✅ **RESOLVED HERE** (planned) | The operation, its grouped-`use` split and the `check --deep` list (Scope). Closed when acceptance tests 1-24 pass |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` | ⚠ **DURING** | Wiring only in `rust.rs` (about 10 lines). All logic in `backends/rust/repoint_facade/`. History row at wrap |
| [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | Same |
| [2026-10-05-restructure-engine-files-past-the-500-line-budget.md](../todo/2026-10-05-restructure-engine-files-past-the-500-line-budget.md) | ⚠ **DURING** (resolved by `tidy-engine-files`) | One `RefactorKind` variant in `plan/refactor_kind.rs` (the kind's home after `tidy-engine-files`), one `mod` + call in `plan/codec.rs`; no field, so `plan.rs` is untouched. All of them stay <= 500 production lines |
| [2026-10-04-restructure-move-item-copies-the-whole-use-header.md](../todo/2026-10-04-restructure-move-item-copies-the-whole-use-header.md), [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md) | — Unrelated | Move-specific; neither is widened. This op is a natural precursor to a move, not a cure for them |
| `packages/tddy-code-restructuring/docs/code-issues/*` others, `broken-restructure-anchors-empty-outline.md` | — | Not in the path |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md); `src/plan/refactor_kind.rs` (variant), `src/plan/codec.rs` (one `mod`, one call), new `src/plan/codec/facade_imports_fields.rs`;
  `src/backends/rust.rs` (wiring); new `src/backends/rust/repoint_facade.rs` + `repoint_facade/{scope,rewrite,group,refusals}.rs`; **visibility-only** widening in `src/crate_move.rs` (`mod header`, `mod manifest_edits`), `src/crate_move/header.rs`,
  `src/backends/rust/item_move/{text,sites}.rs`; `src/runner/rehearsal.rs` and `src/runner/entry_points/check_entry_points.rs` (deep-check notes). Docs at wrap: new `docs/repoint-facade.md`,
  [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md) (a third consumer), [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) (`## Rust operations (v1)`, `## Path survey`, `## Verify`),
  [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md), SKILL.md (operation count, and a line naming this op as the answer to a facade grep).
- **`tddy-tools`, `tddy-index-daemon`**: no source change.

## Related Feature Documentation

- [PRD-2026-10-05-sharpen-repoint-facade.md](../../ft/coder/1-WIP/PRD-2026-10-05-sharpen-repoint-facade.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Path survey`, `## Rust operations (v1)`

## Summary

`repoint_facade_imports`, anchored on a file or a module, rewrites every path whose first hop is a `pub use` (or `use`) of **another crate** inside the file's own crate to the path where the item is **defined**, in `use` items at any depth and in
bodies, splitting a grouped `use` whose members need different qualifiers. Comments, strings and paths to the crate's own items are untouched. `check --deep` prints the list of paths it would rewrite.

## Background

A module that is to move into another crate must name what it uses by its defining crate, or the move presents an edge back to the crate it leaves. `tddy-session-lifecycle`'s `lib.rs` re-exports modules of other crates
(`pub use tddy_daemon_kernel::config;`), so files wrote `crate::config::DaemonConfig`. Twelve files and thirty lines were re-pointed by hand; four paths sat in a grouped `use`, which a text search does not find. The engine already resolves
such a path to its defining crate for `move_module_to_crate`; it has no operation that writes the result without moving anything.

## Responsibility

- New `RefactorKind::RepointFacadeImports` (`repoint_facade_imports`, in `plan/refactor_kind.rs`), codec rules, `SUPPORTED`/`check`/`resolve` wiring (the #584 slice).
- Find each path's defining path with the survey resolver `move-fidelity` makes reachable; rewrite heads in headers and bodies; split a mixed-qualifier group by a fixed rule; keep `as` names; refuse what it cannot rewrite.
- **Text-only**: no server is started or asked anything; plain `check` reports the refusals of a file-anchored op.
- `check --deep` prints the list of paths it would rewrite (a note per path); `apply` prints the same notes.
- Idempotent: a second run over the output rewrites nothing.
- Pin, by test, that `restructure verify` already accounts for the result (F3 decides whether anything more is needed).
- Register the one thin live binary in `.config/rust-e2e.filterset` **and** the `rust-analyzer` group of `.config/nextest.toml`.

## Plan-line schema and the rules (the contract)

```jsonl
{"op":"repoint_facade_imports","anchor":{"kind":"symbol","file":"packages/tddy-session-lifecycle/src/connection_service/agent_roster.rs","path":"agent_roster"}}
{"op":"repoint_facade_imports","anchor":{"kind":"items","file":"packages/tddy-session-lifecycle/src/connection_service.rs","items":["tddy_session_lifecycle::connection_service::agent_roster"],"fingerprints":["sha256:…"]}}
```

The line carries **no other field**; every one (`to`, `name`, `reexport`, `variant`, `type`, `expr`, `order`, `also`, `to_file`, `with_private_deps`, `callee`) is refused as one the operation cannot honour.
**Anchor** (F1): a `symbol` anchor names **one file** (like `move_module_to_crate`; `path` is informational and is not searched), or an `items`/`item` anchor on a module's `mod` declaration names **the module's files**
(`module_files::files_of`, inline modules followed). Plain `check` can examine the first; the second needs `--deep` (it must be lowered).

**1. Which paths are rewritten.** For each file, with `origin = Destination::read(root, <owning package dir>)` and the file's module path from `module_path_of`: `survey_moved_file(workspace, text, &origin, &module_path)`.
A `SurveyedPath` is rewritten iff `defined_at != resolved` **and** `defining_crate != origin.extern_name`. So: a path through a facade that forwards to a foreign crate is rewritten (`crate::config::X`, `self::…`, `super::…`, a chain of facades, a facade
through a path dependency, an explicit `use` of a registry crate); a path to an item the crate defines itself, an **in-crate** facade (`pub use inner::Thing`) and a path already written with a dependency's name are untouched (F2).
The defining path is **`defined_at`**, the walk's textual answer: child module, defined item, explicit `use`, then globs that confirm the name, cycle-safe, across path dependencies. A path the walk cannot see further into comes back as written (no rewrite).
This is deliberately not rust-analyzer's `goto_definition` (the todo's suggestion): the textual resolver is what `move_module_to_crate` already trusts; a macro-generated or registry-internal re-export is not seen, and stays as written.

**2. Preconditions per rewritten path** (refused, naming the path, the file and the line; nothing written): (a) the defining crate is declared by the package's manifest (`[dependencies]`, or either table when the path sits under `#[cfg(test)]`);
(b) the path is spelled on one line without comments inside it; (c) in a **body**, the last segment is unchanged by following the facade (a rename `pub use a::B as C` is refused, F4); (d) the rewrite would not bring a name the same scope already binds (a duplicate `use`, `E0252`: refused, F9).

**3. The edit.** Body path: the written span becomes `defined_at`. A plain `use`: the path becomes `defined_at`, visibility, `as` alias and `;` kept; if the last segment changes the old name is kept `as <old>` (what `keeps_its_name` does).
A `use` group:
- **Rule P (prefix)**: if every leaf agrees on what the group's common prefix becomes, replace the prefix in place: `use crate::config::{self, X};` -> `use kernel::config::{self, X};`; `use crate::{config::A, config::B};` -> `use kernel::{config::A, config::B};`.
  This is exactly the case `repointed_header` handles today, produced the same way.
- **Rule S (split)**: otherwise — the members disagree, the case `repointed_header` refuses (`one_use_per_path`) — each **lifted** member (one with a rewritten leaf) leaves the group and becomes its own statement `<visibility> use <new path>;`; the **kept** members stay in the original statement under the original prefix, **first**, and the lifted statements follow
  in member order, one per line, each with the original indentation. `use crate::{config::Limits, b::Thing};` -> `use crate::{b::Thing};` + `use kernel::config::Limits;` (rustfmt, which `apply` runs over every file it wrote, writes `use crate::b::Thing;`). A group with no kept member disappears into its lifted statements.
  A **nested** group member (`a::{x, y}`) is lifted whole when all its leaves share one new prefix and refused otherwise (F10).
- Refused, never guessed: a statement with an attribute or doc comment directly above it that Rule S would have to split (F3); a member shape that cannot be mapped to its surveyed leaf by its written path; a `use` the scan cannot read as one statement.

**4. Untouched by construction**: comments, doc comments and string literals (the survey reads masked text); own-module and own-crate paths; paths whose head is a foreign crate already; macro-argument paths (not seen: stated in Limits).

**5. Idempotence.** The rewritten path begins with a foreign crate name, so `followed` returns it as written and `defined_at == resolved`: a second run produces an empty edit and the note `nothing in <file> goes through a facade of another crate` (F7: success, not an error).

**6. `check --deep` output.** Every rewrite is a note, printed by `check --deep` and by `apply` through `console::note`:

```
   note: repoint_facade_imports: 4 path(s) in 2 file(s) go through a facade of another crate
   note:   packages/app/src/a.rs:3: crate::config::Settings -> kernel::config::Settings
   note:   packages/app/src/a.rs:5: crate::config::Limits -> kernel::config::Limits (split out of a grouped `use`)
```
(`file:line: written -> defined`, one per surveyed path rewritten, in file and source order.) Refusals are **findings** (non-zero); the list is not (a survey is "expensive, not defective"). Plain `check` prints findings only. The forwarding is `Rehearsed.notes`, filled from `Resolution.notes` (F8).

## Boundaries

- **No facade is written, removed or edited.** The `pub use` that makes a path forward stays; a later `move_*` or a hand edit decides its fate.
- **No manifest edit.** A defining crate the package does not already depend on is a refusal, not a new dependency line (that is the cross-crate moves' manifest pass).
- **No rename of anything.** A facade that renames is kept as `as <old>` in a plain `use` and refused in a body.
- **Only the file's own crate's facades.** A path through *another* crate's `pub use` (`dep::facade::X`) is not followed (`followed` leaves a non-origin path alone); that needs a wrapper over `Walk::follow_absolute` and is left (F2).
- **No change to `followed`, `survey_moved_file`, `repointed_header`, `rewrite_statement` or any move's behaviour.** Only visibility, and `move-fidelity`'s own edits sit beneath this node.
- **No rust-analyzer** in resolution; `apply` still runs its compile gate.
- **No new `verify` rule** unless F3 chooses attribute copying. **No wire change.** **No `RefactorOp` field.**
- Limits recorded in the docs: items a macro generates and paths inside a macro's arguments are not seen; `#[path = "…"]` modules are not followed; `pub(in …)` is not interpreted (all the survey's limits).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| **`tidy-engine-files`** (K=1, `feature/sharpen/tidy-engine-files`) | `plan.rs`, `plan/codec.rs`, `item_anchor.rs` <= 500 production lines by child modules, no behaviour change. In particular `RefactorKind` and its `impl` now live in `plan/refactor_kind.rs` (its decision D1, developer-approved 2026-10-05), reachable at the old path through `pub use`. **File overlap and layout only**: no signature is consumed | the `RepointFacadeImports` variant is added in `plan/refactor_kind.rs`; the codec call sits in the post-split `plan/codec.rs`; `facade_imports_fields.rs` follows the child-module precedent. This node adds **no** `RefactorOp` field, so the 20 struct literals are untouched and `plan.rs` is not edited | re-split them |
| **`move-fidelity`** (K=2, `feature/sharpen/move-fidelity`) | Exactly two lines of visibility in `crate_move.rs`: `mod survey;` and `mod reexports;` become `pub(crate) mod`. That makes the resolver reachable from `backends/rust`: `survey::{survey_moved_file, PathSurvey, SurveyedPath}` and `reexports::followed` (all already `pub(crate)` items inside private modules). Its own use of them is `item_move/canonical_paths.rs`, behind its `canonical_paths` plan field, which this node does not touch. Its B1 edits `sites.rs` `requalified` (module-qualified callers) and does **not** split grouped `use` | **consumes exactly**: `survey_moved_file`, `PathSurvey` and `SurveyedPath` (fields `written`, `resolved`, `defining_crate`, `defined_at`, `in_test`, `in_body`, `site`); `Destination::read` (`pub`) and `Destination::path_dependency` (`pub(crate)`), neither from move-fidelity. **Widened here, visibility only, one line each**, because move-fidelity does not do it: `crate_move.rs` `mod header;` and `mod manifest_edits;` to `pub(crate)` (for `header::{written_prefix, keeps_its_name}` and `manifest_edits::{declares_dependency, Table, position_of, replacement}`; a prefix-agreement helper is split out of `rewrite_of`), `item_move/text::{use_statements, split_use}`, and `sites::members_of` | **re-implement** the walk through `pub use`, globs and path dependencies (`Walk`), the sighting scanner (`source_scan::sightings`), `resolved_against`, `use`-tree member splitting or `declares_dependency`; **change** `followed`'s "origin's paths only" rule or any B1/B2 behaviour; call `repointed_header` (it needs a `Move`) or `rewrite_statement` (it is `Site`-driven); use `canonical_paths` or `defining_paths` |

Not rows, because nothing of theirs is consumed: `retarget-impl` (K=6) and `repoint-call` (K=7) sit below this node on the line. **`retarget-impl`'s `verify` declaration carrier (`verify::compare_with`, `Declared`, `--retarget`, `VerifyRequest.retargets`) is consumed only if decision F3 is taken as "copy attributes" (option (b), not the recommendation)**: the recommended F3(a) refuses, adds no `verify` rule and consumes nothing, so `retarget-impl -> repoint-facade` is **not** an edge unless F3(b) is chosen. The pinned `verify` tests (31-32) call `verify::compare`, which exists on `master` and stays after `retarget-impl`. `repoint-call` adds `--repoint` and `callee`; this node touches neither.

## Draft PR contract

Published with the wave-2 contract commit; **owned surface, new today**:

- `RefactorKind::RepointFacadeImports` (serde `repoint_facade_imports`), added in `plan/refactor_kind.rs`; no `RefactorOp` field (so no struct literal is edited, unlike the three field-adding nodes).
- Crate-private: `backends::rust::repoint_facade::{findings(&RefactorOp, &Workspace) -> Result<Vec<String>>, RustBackend::repoint_facade_imports(&mut self, &RefactorOp, &Workspace) -> Result<Resolution>}`;
  `rewrite::path_edits(text: &str, survey: &PathSurvey, manifest: &str) -> Result<Vec<Rewrite>>` (`Rewrite { written, defined_at, line, split_from_group }`);
  `group::split_or_reprefix(statement: &str, leaves: &[Rewrite]) -> Result<String>`; `Rehearsed.notes: Vec<String>`.
- `Resolution.notes` carries the list in the format of rule 6.
- Failing tests: the thirty red ones in "Acceptance tests" (31-32 are green pins).

## Green wave

**Wave:** 2 of 2.
**Greenable independently:** yes, once `feature/sharpen/tidy-engine-files` and `feature/sharpen/move-fidelity` are on its base.
**Concurrent with:** `feature/sharpen/plan-header`, `feature/sharpen/retarget-impl`, `feature/sharpen/repoint-call` (same wave, no edge between them under the recommendation; the line serialises them because they share `plan/codec.rs`, `backends/rust.rs` and `crate_move.rs`).
**Blocks:** none.
Real dependency edges (whole stack): `tidy-engine-files -> plan-header, retarget-impl, repoint-call, repoint-facade`; `move-fidelity -> repoint-facade`; `retarget-impl -> repoint-call`. Nothing else is an edge: `spawn-record` and `apply-heartbeat` consume nothing and nothing consumes them (`spawn-record` lands after open draft PR #586, a merge-order fact, not a stack edge).
Conditional, not an edge in the list above: `retarget-impl -> repoint-facade` exists only if decision F3 is taken as "copy attributes"; F3 is open and its recommendation (refuse) consumes nothing.

## Successor PRs

None.

## Scope

- [x] **Plan surface**: variant, `facade_imports_fields.rs`, anchors, refused fields
- [x] **Resolution**: `scope.rs` (files), `rewrite.rs` (which paths, preconditions), `refusals.rs`
- [x] **Edits**: body paths, plain `use`, Rule P, Rule S, nested groups, `as` names
- [x] **`check --deep` list**: `Rehearsed.notes`, printed by check and apply
- [x] **Registration**: one live binary in `.config/rust-e2e.filterset` and the `rust-analyzer` group
- [x] **`verify` pinned** by tests (and F3's rule if chosen)
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [x] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [x] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, clippy `-D warnings`, `cargo fmt`; the three engine files <= 500

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `survey_moved_file` (`survey.rs:67`) and `followed` (`reexports.rs:37`) resolve every path to `defined_at`; their modules are private to `crate_move`.
- `repointed_header` (`header.rs:67`) writes the re-point edits for a **move** only, and refuses mixed-qualifier groups, whitespace-spelled paths and tail mismatches.
- `rewrite_statement` (`sites.rs:247`) splits one group level by moved names; no attribute handling.
- `Rehearsal` drops `Resolution.notes`; `check --deep` has no per-path output.
- `verify` pass 4 excuses lowercase-qualifier changes; `use` items are scaffolding; only `#[cfg(test)]` above a `use` is dropped.

### State B (Target)

An op exists that applies the survey's answer to one file or module, by the rules above; `check --deep` lists it; the 12 hand-edited files of the todo would be one plan line each (A4 answered mechanically).

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **New** `repoint_facade.rs` (run: files, origin, survey, edits, notes), `/scope.rs` (~80), `/rewrite.rs` (~150), `/group.rs` (~170), `/refusals.rs` (~100).
- **Widened** (visibility only): `crate_move.rs` `mod header` and `mod manifest_edits` -> `pub(crate)` (`mod survey` and `mod reexports` are `move-fidelity`'s); `header.rs` helpers; `item_move/text.rs` (`use_statements`, `split_use`), `sites.rs` (`members_of`).
- **`rust.rs`**: `mod repoint_facade;`, `SUPPORTED` + one, a `check` arm, a `resolve` arm above `self.start(...)` (no server).
- **`runner/rehearsal.rs`**: `Rehearsed.notes`; **`check_entry_points.rs`**: print each note through `console::note`.
- **`plan/refactor_kind.rs`** (the variant), **`plan/codec.rs`**, **`plan/codec/facade_imports_fields.rs`**: as above. `plan.rs` is not edited.

## Implementation milestones

- [x] **M1** plan surface; tests 1-6
- [x] **M2** resolution and refusals (library level over `fake_lsp`); tests 7-12, 20-24
- [x] **M3** edits: bodies, plain uses, Rule P; tests 13-16
- [x] **M4** Rule S, nested groups, attributes; tests 17-19
- [x] **M5** deep-check notes; tests 25-27
- [x] **M6** thin live binary and registration in both files; tests 28-30
- [x] **M7** `verify` pins; tests 31-32
- [~] **M8** scoped gate and length gate done; package docs staged at wrap

## Testing plan

### Testing Strategy

**Primary: library level with no rust-analyzer** — the op is text-only, so `RustBackend::resolve` and `RustBackend::check` over a `fake_lsp`-backed client (never asked a question) or a static check assert edits and refusals in milliseconds.
**One thin live binary** runs `apply` and `check --deep` through the runner, with `cargo check --all-targets` as the oracle: it is the only place the compile gate, rustfmt and the notes channel are exercised together.

#### Option 1 (chosen): library level, fixtures in `tests/facade_imports/mod.rs`
Fixture: workspace of `kernel` (defines `config::{Settings, Limits}`, `paths`), `agents` (defines `roster`), `mid` (re-exports `kernel::config` — a facade chain) and `app` (depends on all; `lib.rs` holds `pub use kernel::config; pub use kernel::paths as user_paths; pub use agents::roster::*; pub use inner::Thing;`, files `a.rs`, `b.rs`).
**Trade-off**: fast and exact on text; does not prove the tree compiles (the live binary does).
**Location**: `packages/tddy-code-restructuring/tests/repoint_facade_imports_acceptance.rs` (not in the e2e filterset: no rust-analyzer).

#### Option 2 (chosen, thin): live fixture crate
**Location**: `packages/tddy-code-restructuring/tests/repoint_facade_imports_live_acceptance.rs` (new; `.config/rust-e2e.filterset` and the `rust-analyzer` group in `.config/nextest.toml`).

#### Option 3 (rejected): rust-analyzer's `goto_definition` as the resolver
Would add a server wait to a text edit and disagree with the resolver `move_module_to_crate` trusts.

### Coverage Requirements

- [ ] Happy: plain `use`, group (P and S), glob, body, chain of facades, path dependency, registry crate
- [ ] Refusals: undeclared crate, rename in a body, whitespace-spelled path, duplicate binding, attribute on a split group, nested group with disagreeing leaves
- [ ] Untouched: comments, doc comments, strings, own paths, in-crate facade, foreign-named paths
- [ ] Idempotence, module vs file scope, `check` parity with `resolve`
- [ ] Actual effects: bytes on disk; `cargo check --all-targets`

## Acceptance tests

Names read as behaviour specifications. Items 1-27 and 28-30 are **red on `master`** (`unknown variant repoint_facade_imports`); 31-32 are **green pins** (they specify what already holds).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/repoint_facade_imports_acceptance.rs` (new; library level, `fake_lsp`-backed backend, no rust-analyzer)

1. `a_line_carrying_only_an_anchor_is_a_plan_and_every_other_field_is_refused` (table of the refused fields).
2. `a_symbol_anchor_names_one_file_and_an_items_anchor_names_a_module_and_a_range_anchor_is_refused`.
3. `a_plain_use_through_a_facade_is_re_pointed_to_the_crate_that_defines_it` — `use crate::config::Settings;` -> `use kernel::config::Settings;`.
4. `a_path_in_a_body_is_re_pointed_and_the_rest_of_the_line_is_untouched` — `crate::config::Settings::default()`.
5. `a_glob_import_through_a_facade_is_re_pointed` — `use crate::config::*;`.
6. `a_chain_of_facades_is_followed_to_the_defining_crate` (`app` -> `mid` -> `kernel`) and `an_explicit_use_of_a_registry_crate_is_re_pointed`.
7. `a_path_to_an_item_the_crate_defines_itself_an_in_crate_facade_and_a_path_already_written_with_a_dependencys_name_are_left_alone` (`crate::b::Thing`, `self::`, `super::`, `crate::Thing` via `pub use inner::Thing`, `kernel::config::X`).
8. `a_path_in_a_comment_a_doc_comment_or_a_string_is_left_alone` — byte-identical lines.
9. `a_defining_crate_the_package_does_not_depend_on_is_refused_naming_the_path_and_the_crate_and_nothing_is_written`.
10. `a_dev_dependency_is_a_target_only_for_a_path_under_cfg_test`.
11. `a_facade_that_renames_keeps_the_name_with_as_in_a_use_and_is_refused_in_a_body`.
12. `a_path_spelled_across_whitespace_or_a_comment_is_refused_not_guessed`.
13. `a_group_whose_members_agree_gets_its_prefix_re_pointed_in_place` (Rule P: `use crate::config::{self, X};`, `use crate::{config::A, config::B};`).
14. `a_group_whose_members_disagree_is_split_kept_members_first_then_one_use_per_lifted_member` (the todo's `use crate::{config::Limits, b::Thing};`; exact text before rustfmt).
15. `a_group_whose_every_member_goes_through_a_facade_becomes_one_use_per_member`.
16. `a_nested_group_member_is_lifted_whole_when_its_leaves_agree_and_refused_when_they_do_not`.
17. `an_attribute_or_doc_comment_above_a_group_that_would_split_is_refused_and_above_a_plain_use_is_kept`.
18. `a_rewrite_that_would_bind_a_name_the_scope_already_binds_is_refused_naming_both`.
19. `the_split_keeps_the_visibility_and_the_indentation_of_the_statement_it_replaced` (a `pub use` group; a `use` inside `mod tests`).
20. `running_the_operation_on_its_own_output_rewrites_nothing_and_says_so` — empty edit, note `nothing in … goes through a facade of another crate`.
21. `a_module_anchor_covers_every_file_of_the_module_and_a_file_anchor_only_its_own`.
22. `a_static_check_of_a_file_anchored_plan_reports_the_refusals_resolve_reports` (parity: tests 9, 11, 12, 18 through `RustBackend::check`).
23. `a_static_check_of_a_module_anchored_plan_says_to_run_deep`.
24. `the_edits_are_the_same_whether_the_file_was_read_from_disk_or_through_an_earlier_operations_overlay`.

### `tddy-code-restructuring` — deep-check output, same file

25. `a_deep_check_note_lists_every_path_it_would_rewrite_with_file_line_written_and_defined` — the exact lines of rule 6, in file and source order, with `(split out of a grouped `use`)` where it applies. *Fails today*: unknown operation, and `Rehearsal` drops notes.
26. `a_deep_check_note_for_a_clean_file_says_nothing_goes_through_a_facade_and_is_not_a_finding`.
27. `a_deep_check_forwards_the_notes_of_every_operation_and_changes_no_finding` — a `move_item` plan's deep check now also prints its notes; findings unchanged. *Fails today*: notes dropped.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/repoint_facade_imports_live_acceptance.rs` (new live binary, `assert_compiles_with_its_tests`)

28. `a_file_of_facade_imports_is_re_pointed_and_the_workspace_still_compiles_with_its_tests` — three-crate fixture, applied through the runner; the result is rustfmt-clean (`is_rustfmt_clean`).
29. `a_deep_check_lists_what_an_apply_then_rewrites_and_writes_nothing_itself` — the account lines equal the apply's notes; tree unchanged after the check.
30. `a_module_anchored_run_re_points_every_file_of_the_module_and_leaves_siblings_alone`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/verify_accounts_for_facade_re_points.rs` (new; library level over `verify::compare`)

31. `a_body_path_and_a_called_function_re_pointed_through_a_facade_are_accounted_for_and_counted_as_repointed` — **green today** (pass 4); pins it.
32. `a_split_group_changes_nothing_verify_reads_and_a_renamed_item_is_still_reported` — green today; the second half is why the op refuses body renames.

### Contract commit: what was published, and what the tests showed (wave 2)

All F1-F11 were taken as **recommended**; nothing in the tests contradicts one.

- **Published**: `RefactorKind::RepointFacadeImports` in `plan.rs` (`plan/refactor_kind.rs` does not exist on this base), `SUPPORTED` entry, `plan/codec/facade_imports_fields.rs` (`refuse_a_facade_repoint_it_cannot_honour`, **not implemented**: accepts everything, so test 1 fails), `backends/rust/repoint_facade.rs` with `findings` (empty, on purpose: a static finding makes `check_plan` skip `resolve`, which is where the refusal naming this node is reported) and `RustBackend::repoint_facade_imports` (refuses `UnsupportedOp` naming `repoint-facade`), `repoint_facade/{scope,rewrite,group,refusals}.rs` skeletons with the contract signatures (`scope::files_of`, `rewrite::{Rewrite, path_edits}`, `group::split_or_reprefix`, `refusals::unfinished`), `Rehearsed.notes` (field and printing in `check_entry_points.rs` are wired; **filling it from `Resolution.notes` is the TODO**, so existing deep checks print nothing new yet), registration of the live binary in both config files, and `harness::checking_the_plan_with` (an `adjust` hook, as `applying_the_plan_with` has).
- **Not widened**: the `crate_move` visibility lines (`header`, `manifest_edits`, `text::{use_statements, split_use}`, `sites::members_of`) belong to green; the skeleton uses none of them.
- **Correction, test 23**: plain `check` already reports `... anchors by item, which only a deep check can resolve ... run \`check --deep\`` for every item-anchored operation, so test 23 is a **green pin** of that generic finding (it names `RepointFacadeImports`), like 31-32.
- **Correction, test 27**: a `move_item` deep check needs a live server, which the library-level file does not have. It is written over **two `repoint_facade_imports` operations** (notes of every operation, in operation order, no finding added). The `move_item` half ("notes already produced by other operations become visible") is not pinned by any test; green should add it to the live binary if wanted.
- **Interpretation, test 2**: an `items` anchor on a `mod` declaration is lowered by the server to a `range` over that declaration, so the library tests anchor a module by that range (`facade_imports::a_repoint_of_the_module`); "a range anchor is refused" means a range that covers no `mod` declaration.
- **Layout**: the module is `backends/rust/repoint_facade/` as this changeset names it (the node brief said `repoint_facade_imports/`).
- **`verify` pins (31-32) were run and pass** as written: the re-point pass pairs the two body statements (`excused.repointed == 2`), a split group is invisible (a `use` is scaffolding), and a renamed item stays a lost and a gained statement. `is_structural`'s `impl ` handling is irrelevant here.
- **Unit tests** (inline, fail for the missing implementation): `group.rs` (Rule P, Rule S, no kept member) and `rewrite.rs` (rewrite condition, undeclared crate, own paths).
- **Fixture assumptions green must confirm**: the library tests assume the textual resolver follows `app` -> `mid` -> `kernel` (a path dependency chain), reports a body path as `crate::config::standard_limits` (no call parentheses), and returns `regcrate::Clock` for a `pub use` of a registry crate it cannot read.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (stack brief, quoted): "8-node decomposition approved (2026-10-05)."; "Prep node: ADD the mechanical node first."; "Log-history fix is its own PR #586 — NOT in this stack."

**OPEN** (each with a recommendation):
- **F1 — how a file is anchored.** (a) **`symbol` anchor for a file + `items` on a `mod` declaration for a module** — *recommended*: no new anchor kind, plain `check` works for files; (b) a new `file` anchor kind (touches `Anchor`, the ledger, the plan store, codec, lowering: far too wide); (c) `items` only (cannot name `lib.rs`).
- **F2 — which facades.** (a) **a facade in the file's own crate that forwards to a foreign crate** — *recommended*, the todo's case; (b) also in-crate facades (`pub use inner::Thing`): changes the crate's own paths, a different intent; (c) also `dep::facade::X` through a dependency's re-exports: needs a wrapper over `Walk::follow_absolute` (`followed` leaves non-origin paths alone) — a follow-up.
- **F3 — a group Rule S must split that carries an attribute or doc comment.** (a) **refuse** — *recommended*: nothing to teach `verify`, no lost gate; (b) copy the attribute onto every statement and extend `verify` (only `#[cfg(test)]` above a `use` is dropped today, so a duplicated `#[cfg(feature)]` would read as gained) through `retarget-impl`'s `Declared`. **Choosing (b) is what makes `retarget-impl -> repoint-facade` a real edge** (the carrier is consumed); under (a) there is no such edge.
- **F4 — a facade that renames, in a body.** (a) **refuse** — *recommended*: the token change would be neither a re-point `verify` excuses nor one a reader expects; (b) rewrite to the defining name.
- **F5 — a defining crate the package does not depend on.** (a) **refuse, naming it** — *recommended*; (b) skip and report (the author thinks it complete); (c) rewrite to the nearest direct-dependency hop; (d) add the manifest line (that is the move operations' manifest pass).
- **F6 — shape of a split.** (a) **one statement per lifted member, kept group first** — *recommended*, matches `rewrite_statement` and the todo; (b) regroup lifted members by shared new prefix (fewer lines, less predictable).
- **F7 — nothing to rewrite.** (a) **success with a note** — *recommended*, idempotence; (b) refuse.
- **F8 — carrying the list to `check --deep`.** (a) **forward every operation's `Resolution.notes` into the deep check** — *recommended*, general (test 27 pins the side effect on `move_item` and `reparent_module` notes, which become visible); (b) a typed `Resolution` field shown only for this op; (c) special-case this kind in `Rehearsal` (the `survey` precedent, but a branch per op).
- **F9 — a rewrite that would bind a name twice.** (a) **refuse** — *recommended*; (b) drop the now-redundant `use`; (c) leave it to the compile gate (`E0252` after the edit).
- **F10 — nested groups.** (a) **lift whole when leaves agree, refuse otherwise** — *recommended*, as `rewrite_statement` refuses nested groups; (b) full recursion.
- **F11 — textual resolver vs rust-analyzer.** (a) **textual (`defined_at`)** — *recommended*; (b) `goto_definition` as the todo suggests: sees macro-generated re-exports, costs a server wait and can disagree with the move operations.

Decisions taken by this plan: no `RefactorOp` field; the op starts no server; no manifest edit; the rewrite condition is `defined_at != resolved && defining_crate != origin`.

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

(empty; populated by `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`)

## TODO

- [x] Record initial discovery (`2026-10-05-sharpen-repoint-facade-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-05-sharpen-repoint-facade.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-05-sharpen-repoint-facade-initial-discovery.md`, and the facade-import todo if it has reached `master` (else whichever of #532 and this PR lands second deletes it)
- [ ] USER REVIEW — work complete, decide next steps

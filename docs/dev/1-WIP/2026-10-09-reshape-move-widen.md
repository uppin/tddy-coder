# Changeset: cross-crate moves widen what the origin still reaches, and say so

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (engine: visibility edits in the files a cross-crate move carries; report and `check --deep` output)
**Stack**: `#reshape` 7/19, branch `feature/reshape/move-widen`, wave 1. PR title:
`feat(code-restructuring): cross-crate moves widen what the origin still reaches and say so (#reshape 7/19)`.
Base in the linear stack: `feature/reshape/methods-leave-type` (K=6). **Real edges**: none. This node consumes no
behaviour of any other node, and no node of this stack consumes its behaviour. It sits on the line only because `gh stack`
needs a line. It shares `crate_move/cluster.rs`, `crate_move.rs` and the `backends/rust.rs` wiring with nodes 1, 5 and 8,
so the collisions are textual only.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-move-widen-initial-discovery.md)
(Exploration 1 is the whole-work discovery; Exploration 2 is this node's).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds every record **unclaimed**:
**no 🚧 claimed issue is in the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md](../todo/2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md) | ✅ **RESOLVED HERE** | A reached module-level item is widened to `pub` and reported (rules 1, 6). Deleted at wrap when acceptance tests 1 and 19 pass |
| [2026-10-08-restructure-move-to-crate-leaves-pub-crate-items-the-origin-still-uses.md](../todo/2026-10-08-restructure-move-to-crate-leaves-pub-crate-items-the-origin-still-uses.md) | ✅ **RESOLVED HERE** | Fields, inherent members, inline-module items and child `mod` declarations are widened too, by position. Trait-impl members are never touched (rules 1, 4). Deleted at wrap when tests 2–8, 19, 21, 23 pass. Tuple-struct fields go to the new todo below |
| [2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md](../todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md) | ✅ **RESOLVED HERE** | Glob-visible items count as reached (rule 2), and the `check --deep` survey line names the glob (rule 7). Deleted at wrap when tests 9–11, 20, 25 pass |
| [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) | ✅ **RESOLVED HERE** (item 3 only) — this node is the claimant | Item 3 ("the widening a cross-crate move forces is not reported") is closed by rules 1 and 6–7. **The wrap narrows the entry and does not delete it.** Item 1 (`move_items_to_crate`) stays open and is deferred. Item 2 belongs to `feature/reshape/apply-robust` and item 4 to `feature/reshape/move-grouped-use`; the wrap marks each closed only if that node has already landed |
| [2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses.md](../todo/2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses.md) | — Reference | Not fixed: no narrowing pass. Reach-driven, positional widening stops this debt from growing (test 3, test 6) |
| [2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md](../todo/2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md) | — Unrelated (node 1's claim) | Same symptom for same-crate moves, different mechanism (`Scope` arithmetic). Nothing shared but the shape of a positional visibility edit |
| [2026-10-09-restructure-moves-do-not-widen-tuple-struct-fields.md](../todo/2026-10-09-restructure-moves-do-not-widen-tuple-struct-fields.md) (**new, written by this node**) | ⚠ **DEFERRED** (new todo) | rust-analyzer's outline names tuple fields `0`, `1`, which no identifier filter keeps. Covers same-crate and cross-crate moves; node 1 references it |
| [oversized-file-backends-rust.md](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md), [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | `rust.rs` net growth ≤ 0: the declaration walk is a new `backends/rust/declarations.rs`, and `outside_references_opening` calls it in place of `path_reached_within`. The two `Resolution::of(…)` wrappers go away (`:1286`, `:1328`) |
| Function-size list (whole-work discovery, Exploration 3): `resolve_cluster` 66, `check_plan` 115, `resolve_opening` 163 | ⚠ **DURING** (owned by `feature/reshape/fn-sizes-rest`, `feature/reshape/fn-sizes-backend`) | None of them grows. `resolve_cluster`'s body becomes `cluster_edits` with an unchanged line count. `check_plan`'s survey and notes block moves into a new `account_rehearsal`, so it shrinks. `resolve_opening` loses two wrappers |
| [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md), `pub(in crate::<origin module>)` rewrite | — Unrelated (node 8's) | Developer decision 2026-10-09: node 8 rewrites `pub(in …)` in crate moves. This node only widens a **reached** declaration to `pub`, whatever it was written as |
| `packages/tddy-code-restructuring/docs/code-issues/*` others | — | Not in the path |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md);
  - `src/crate_move.rs`: the `ModuleReferences` contract, `ItemReferences` fields, `DeclarationKind`, `Survey.reexported_by`, `resolve` returns `Resolution`, `mod widening`;
  - `src/crate_move/cluster.rs`: `resolve_cluster` → `cluster_edits`, plus a new `resolve_cluster`;
  - new `src/crate_move/widening.rs`, `widening/{reach,declaration,escaping,glob}.rs`;
  - new `src/backends/rust/declarations.rs`; `src/backends/rust.rs` (wiring only);
  - `src/edit.rs` (`VisibilityChange.reason`) and the 7 `VisibilityChange` literals;
  - `src/console.rs` (`widening` states the reason);
  - `src/runner/rehearsal.rs` (`Rehearsed.report`, the glob line in `survey_lines`);
  - `src/runner/entry_points/check_entry_points.rs` (`account_rehearsal`);
  - `tests/harness/mod.rs` (`resolution_of`, two fixtures);
  - two new test binaries.
  - Docs at wrap: [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md) (what counts as reached, the glob line), [facades.md](../../../packages/tddy-code-restructuring/docs/facades.md) (glob re-exports and widening), [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) (`## Rust operations (v1)` rows, `## Path survey`, `## Known limitations`), [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (the cross-crate rows: "visibility widenings are reported").
- **`tddy-index-daemon`**, **`tddy-tools`**: no source change. Both state widenings through `console::widening` (`tddy-index-daemon/src/apply.rs:327`), so the reason reaches them with no edit.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-move-widen.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-move-widen.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md): `## Rust operations (v1)`, `## Path survey`, `## Known limitations`

## Summary

A cross-crate move makes `pub` every declaration in the files it carries that a file it does not carry, outside the
destination crate, still reaches: module-level items, items of inline modules and the `mod` declarations on their path,
child `mod` declarations, struct fields, and inherent methods and consts. It also widens every non-private item the
moved module's parent re-exports by glob, and every type a widened signature names. Edits are addressed by the
declaration position rust-analyzer reports. Every widening is reported by `apply` and the journal, and now also by
`check --deep`, before anything is written.

## Background

On `#carve` 21/21 each cross-crate milestone ended in a `cargo check` loop, with 275 hand widenings from `pub(crate)` to
`pub`. The loops made two mistakes of their own: a name-based rewrite widened same-named parameters, and `pub` was
written onto a trait method (E0449). The engine already knows the reached set: `crate_move::surveyed` computes it, and
`check --deep` prints "1 item(s) reached from outside: mint_first_admission_token". Nothing then acts on it, because
`crate_move/` has no visibility edit. `check --deep` drops the widening report of every operation (`Rehearsal` keeps
`Resolution.notes` only). Across a crate boundary there is no visibility narrower than `pub`, so "as little as needed"
means widening **only what is reached**.

## Responsibility

- Extend the `ModuleReferences` contract so it returns **every declaration** of a moving file. Each comes with its
  declaration position, the containers it sits in, and its kind. The Rust implementation reads them from the
  `documentSymbol` tree.
- Decide what to widen (rules 1–4) at library level, with no server, over the fake reference set.
- Write the widenings into the moving files' existing changes, one change per file, in pre-move coordinates.
- Return a `Resolution` with the report from `crate_move::resolve` and `resolve_cluster`. Carry `reason` on
  `VisibilityChange`.
- `check --deep` prints the report of every rehearsed operation and names the parent glob on the survey line.
- Register the one new live binary in `.config/rust-e2e.filterset` and the `rust-analyzer` group of `.config/nextest.toml`.
  The library-level binary needs no server and is not registered.

## The rules (the contract)

There is no plan-line change. `move_module_to_crate`, `move_cluster_to_crate` and `move_test_binary_to_crate` carry the
fields they carry today, and a test-binary move widens nothing.

Definitions: the **moving files** are the members' files (`Survey.source`, and every file the set carries if
`feature/reshape/move-children` has added children by then). An **outside reference** is one whose `path` is not a moving
file **and** is not under the destination's `dir`. A reference from inside the destination crate needs nothing: before
the move it was already crossing a crate boundary, so the declaration is already `pub`.

**1. Reach.** A declaration whose `kind` is `Item`, `Field` or `InherentMember` and that has at least one outside
reference is **reached**. A reached `Item` inside inline modules also reaches each inline `mod` on its path (`within`).

**2. Glob-visible.** For each member, `parent_reexports_of(<declared_in text>, <module>)` is read
(`facade_writer.rs:236`). It finds the top-level `use <module>::*;` lines at any visibility. When one exists, every
top-level `Item` of the member that is not written private is reached, with reason
`` through `<declared_in file>`'s glob ``. A private item is not visible to the glob and stays as written. Nested
`use <module>::{a, b}` groups are left to rule 1: their leaves are references.

**3. Escaping types.** For each `Item` or `Field` that rules 1–2 widen, the signature is read from the moving file's
masked text. For a `fn`, that runs from the declaration position to the first `{` or `;` at depth 0. For a field it is
the type. A `struct`, `enum`, `type` or `trait` of a moving file whose name is an identifier token there is reached, with
reason `` named by the signature of `<item>` ``. This repeats until nothing new is reached; a widened struct's fields
count as signatures only when they are widened themselves. The rule is lexical and deterministic, like
`escaping_types::escaped_names`. A shadowing type of the same name from elsewhere is a known false positive, and it only
widens.

**4. Never touched.**
- `kind == NoVisibility`: members of an `impl <Trait> for <T>`, the items of a `trait`, and enum variants.
- A declaration whose visibility is already `pub`.
- A declaration nothing reaches.
- An unreached `pub(in …)` or `pub(super)`. Developer decision 2026-10-09: rewriting `pub(in crate::<origin module>)` is
  `feature/reshape/move-grouped-use`'s. This node never respells a restricted visibility. It replaces it with `pub` only
  when the declaration is reached.

**5. The edit.** At `declared_at`, the visibility keyword in front of the name is replaced (`pub(crate) `,
`pub(super) `, `pub(in a::b) `, or nothing, after any attributes and `async`/`const`/`unsafe` qualifiers on the same line)
with `pub `. Edits are merged into the member's existing `FileEdit::Change` for `Survey.source`, in pre-move coordinates,
like the header edits (`cluster.rs:139`); otherwise a new change is pushed. A declaration whose keyword is on a line above
its name is **refused** as `SeamRefused`:
`` the declaration of `<name>` in `<file>` has its keyword on a line above its name, so the move cannot widen it: write the keyword and the name on one line ``.
Nothing is written. The same refusal exists for `move_item` (`item_move/outline.rs:209-213`).

**6. The report.** One `VisibilityChange` per widening, in moving-file order and then source order:
- `item`: the name, qualified by its containers. A member is `Type::member` (`AttachmentState::config`,
  `AgentRoster::broadcast`); an inline-module item is `inner::X`.
- `from`: the written visibility (`private` for none). `to`: `pub`.
- `reason`: `None` under rule 1, `Some(…)` under rules 2–3.

`console::widening` states it as `` `AgentRoster::broadcast` pub(crate) -> pub `` and appends ` (<reason>)` when there
is one. `apply` prints it as `   visibility: …` (`runner/entry_points.rs:218-227`), and the journal records it
(`runner.rs:94`). A journal written before `reason` existed reads it as `None`
(`#[serde(default, skip_serializing_if = "Option::is_none")]`).

**7. `check --deep`.** `Rehearsed.report` is filled from `Resolution.report`. `account_rehearsal` prints, per operation:
the survey lines, then one `   visibility: …` line per widening, then the notes. A widening is not a finding, so the exit
code is unchanged. When a surveyed member has a parent glob (`Survey.reexported_by`), `survey_lines` adds
`      reached through a glob re-export, not by path: <declared_in>:<line>: <statement>`. That is the line that
explains "0 caller(s)" beside reached items.

## Boundaries

- **Visibility keywords only.** No caller, facade, manifest, `use` header or path is changed beyond what the moves do
  today. No refusal is added apart from rule 5's.
- **No respelling of `pub(in …)`.** That belongs to `feature/reshape/move-grouped-use` (developer decision).
- **No narrowing.** Nothing already `pub` becomes narrower, and the hand widenings from `#carve` stay.
- **Not widened, and recorded as limits:**
  - tuple-struct fields (new todo);
  - `macro_rules!` and `use` re-exports inside a moved file, which `documentSymbol` does not list as widenable items;
  - associated types of an inherent impl (unstable);
  - an item that only a macro expansion in the origin names.
  The apply's compile gate surfaces these, as it does today.
- **Same-crate moves and `extract_module` keep their own rules.** They gain only `reason: None` on their report literals,
  plus the side effect that `check --deep` now shows their widenings.
- **No new plan field, flag, wire message or `RefactorKind`.**

## Dependencies

This node has no parent: it consumes nothing from nodes 1–6, and its base `feature/reshape/methods-leave-type` is
sequential only.

## Draft PR contract

Published with the wave-2 contract commit, the first push of this PR. **Owned surface, new today**:

- **`ModuleReferences` contract change** (`crate_move.rs`). The method signature is unchanged:
  `fn outside_references(&mut self, workspace: &Workspace<'_>, file: &str) -> Result<Vec<ItemReferences>>`. It now
  returns **every declaration** of `file`, not only top-level items. `ItemReferences` gains three public fields:
  `declared_at: Position` (the name, one-based, character column), `within: Vec<String>` (containers outermost first: an
  inline module's name, `impl T`, `impl Tr for T`, a struct or enum name), and `kind: DeclarationKind`. Plus a new type:
  `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DeclarationKind { Item, Field, InherentMember, NoVisibility }`.
  `surveyed` keeps its behaviour by reading only `kind == Item && within.is_empty()`.
- `pub fn crate_move::resolve(engine: &mut dyn ModuleReferences, workspace: &Workspace<'_>, op: &RefactorOp) -> Result<Resolution>`
  and `pub fn resolve_cluster(engine, workspace, cluster: &MovingCluster) -> Result<Resolution>`; both returned
  `WorkspaceEdit` before. Also `fn cluster_edits(…) -> Result<(WorkspaceEdit, Vec<Survey>)>` (private).
- `Survey` gains `pub declarations: Vec<ItemReferences>` (every declaration of the member, as surveyed) and
  `pub reexported_by: Option<String>` (`<file>:<line>: <statement>`).
- `pub(crate) fn crate_move::widening::widened(workspace: &Workspace<'_>, cluster: &MovingCluster, edit: WorkspaceEdit, surveys: &[Survey]) -> Result<Resolution>`.
- `pub(crate) fn widening::declaration::visibility_span(text: &str, declared_at: Position) -> Result<Option<(std::ops::Range<usize>, String)>>`
  (refuses rule 5). `pub(crate) fn widening::escaping::escaping(texts: &BTreeMap<String, String>, widened: &[Widened], candidates: &[Widened]) -> Vec<Widened>`.
- `pub(super) fn backends::rust::declarations::declarations_within(symbols: &Value) -> Vec<Declared>`
  (`Declared { name, within, position: Value, kind: DeclarationKind }`).
- `VisibilityChange.reason: Option<String>` (serde default, skipped when `None`). `console::widening` states it.
- `Rehearsed.report: Vec<VisibilityChange>`. `fn account_rehearsal(account: &ProgressSink, index: usize, rehearsed: &Rehearsed)`
  in `check_entry_points.rs`.
- Harness: `pub async fn resolution_of(fixture: &AFixtureWorkspace, op: RefactorOp) -> Result<Resolution, String>`,
  `a_workspace_whose_module_the_origin_still_reaches()`, `a_workspace_whose_nested_module_its_parent_globs()`.
- Failing tests: the 26 under "Acceptance tests".

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes. Nothing below it on the line is consumed.
**Concurrent with:** every other wave-1 node: `feature/reshape/widen-same-crate`, `multi-seam-extract`, `tidy-facades`,
`extract-method-clean`, `move-children`, `methods-leave-type`, `move-grouped-use`, `new-crate`, `apply-robust`,
`move-item-paths`, `anchors-outline`.
**Blocks:** none.
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`, `4→19`,
`17→19`. None touches this node.

## Successor PRs

None in this stack depends on it. The next branch on the line, `feature/reshape/move-grouped-use`, is sequential only.
It also owns the `pub(in …)` rewrite that this node leaves alone.

## Scope

- [ ] **Reference seam**: `ItemReferences` fields, `DeclarationKind`, `declarations.rs`, the fake updated, `surveyed` filtered
- [ ] **Deciding step**: rules 1–4 in `widening/{reach,glob,escaping}.rs`
- [ ] **Edits and report**: rule 5 (`declaration.rs`), merge, `Resolution`, `VisibilityChange.reason`, `console::widening`
- [ ] **`check --deep`**: `Rehearsed.report`, `account_rehearsal`, the glob survey line
- [ ] **Registration**: the live binary in `.config/rust-e2e.filterset` and the `rust-analyzer` group
- [ ] **Package documentation** at wrap (list under Affected Packages); the new todo committed with the planning commit
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, clippy `-D warnings`, `cargo fmt`; `rust.rs` net ≤ 0; no function on the size list grows; every new function ≤ 40 lines

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `ModuleReferences::outside_references` (`crate_move.rs:73-84`) returns top-level items only: `ItemReferences { item, referenced_at }`
  (`:86-93`), with no declaration position, kind or members. The Rust implementation (`backends/rust.rs:1449-1482`) skips
  inline-module items (`:1471`), and `path_reached_within` (`:2692-2747`) never descends below a module.
- `surveyed` (`crate_move.rs:211-267`) computes `reached_from_outside`. It is used only by `survey_lines` and the named facade.
- `resolve_cluster` (`cluster.rs:111-176`) returns `WorkspaceEdit`. The backend wraps it as `Resolution::of` (`rust.rs:1286`, `:1328`),
  so the report is always empty. No file under `crate_move/` edits a visibility.
- `Rehearsal::rehearse` (`runner/rehearsal.rs:52-77`) drops `Resolution.report`. `check_plan` (`check_entry_points.rs:295-309`)
  prints survey lines and notes only.
- `VisibilityChange { item, from, to }` (`edit.rs:55-59`); `console::widening` (`console.rs:309-311`).

### State B (Target)

- A crate move's `Resolution` carries a `pub` edit and a report line for every declaration rules 1–3 reach, and none for
  anything else.
- `check --deep` prints the same lines before the apply.
- The R3 and 09-25 shapes compile after the move with no hand edit.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **`crate_move.rs`**: `DeclarationKind`; three `ItemReferences` fields; `Survey.{declarations, reexported_by}`;
  `surveyed` filters items, keeps the full list on the survey, and calls a new one-line `glob::reexport_of(workspace, moving)`;
  `resolve` returns `resolve_cluster(…)` directly; `mod widening;`. The fake `AKnownReferenceSet` and `references_to`
  fill the new fields (top-level `Item`, `within: []`).
- **`crate_move/cluster.rs`**: the body of `resolve_cluster` is renamed `cluster_edits` and returns `(edit, surveys)`
  (same line count). A new 4-line `pub fn resolve_cluster` calls it, then `widening::widened`.
- **New `crate_move/widening.rs`** (~90: `widened`, the merge into one change per file, report order) and
  **`widening/reach.rs`** (~90: rules 1 and 4, outside-reference test, `within` → inline `mod` declarations),
  **`widening/glob.rs`** (~50: rule 2, `reexport_of`), **`widening/escaping.rs`** (~80: rule 3 fixpoint over masked
  text from `source_scan`'s `masked`, widened from private to `pub(crate)`, visibility only), **`widening/declaration.rs`**
  (~70: `visibility_span`, rule 5 refusal, `Position` → byte offset via `apply::byte_offset`, `apply.rs:138`).
- **New `backends/rust/declarations.rs`** (~80): walks the `documentSymbol` tree, keeping identifier names, and builds
  `within` with each container's own name (`impl T`, `impl Tr for T` read from the symbol name). Kind: a child of an
  `impl … for …` holder, of a `trait` (kind 11) or of an `enum` (kind 10) is `NoVisibility`; a child of a struct
  (kind 23) is `Field`; a child of an inherent `impl` is `InherentMember`; everything else is `Item`.
- **`backends/rust.rs`**: `outside_references_opening` iterates `declarations_within` instead of `path_reached_within`,
  dropping the `within` skip, and fills the three fields. The two `Resolution::of(…)` wrappers go. `mod declarations;`.
- **`edit.rs`** `VisibilityChange.reason`; the 7 literals (`visibility.rs:86`, `facade.rs:42`,
  `module_reparent/visibility.rs:65`, `item_move/assemble.rs:247,268`, `console.rs:384`, one test) gain `reason: None`.
- **`console.rs`** `widening` appends ` ({reason})`.
- **`runner/rehearsal.rs`** `Rehearsed.report`; the glob line in `survey_lines`. **`check_entry_points.rs`**
  `account_rehearsal`, called once from `check_plan` in place of its survey and notes block.

## Implementation milestones

- [ ] **M1** contract: `DeclarationKind`, fields, fake, `surveyed` filter, `Resolution` return types, `reason`; library tests 16–18 compile and fail on behaviour
- [ ] **M2** reach and exclusions (rules 1, 4) with positional edits (rule 5); tests 1–8, 13–15
- [ ] **M3** glob-visible (rule 2); tests 9–11
- [ ] **M4** escaping types (rule 3); test 12
- [ ] **M5** cluster merge; test 16
- [ ] **M6** `declarations.rs` and the backend wiring; live tests 19–23. **Record** in Validation Results the number of extra `textDocument/references` requests and the wall time for the R6-shape fixture and for a re-run of #carve 21 R3's module on a warm index (measured, not bounded)
- [ ] **M7** `check --deep`: `Rehearsed.report`, `account_rehearsal`, glob line; tests 24–26
- [ ] **M8** registration, docs staged, scoped gate, size checks

## Testing plan

### Testing Strategy

**Primary: library level with no server.** `crate_move::resolve_cluster` runs over a temporary workspace and a fake
`ModuleReferences` that answers a known declaration set. That is the precedent of `crate_move.rs`'s own
`AKnownReferenceSet` and `tests/cluster_move.rs`. Every rule's decision and the exact edited text are asserted in
milliseconds.

**One live binary** proves the half a fake cannot. rust-analyzer's `documentSymbol` tree yields the kinds and positions
the edits address, and the result compiles: `assert_compiles`, `assert_lints_clean`. It is also the only place
`check --deep` and `apply` run end to end.

**Unit tests (not acceptance)**: `backends/rust/declarations.rs` over recorded `documentSymbol` JSON (kinds, `within`,
the tuple-field `0` skipped); `runner/rehearsal.rs` `survey_lines` with `reexported_by`.

#### Option 1 (chosen): library level, `packages/tddy-code-restructuring/tests/move_to_crate_widening.rs`
Fixture: `origin` (`lib.rs`: `pub mod roster; pub mod runtime; pub mod connection_service;`) and `destination` (empty
root). `roster.rs` holds `pub(crate) struct AgentRoster { pub(crate) rev: u64, secret: u64 }`, an inherent `impl` with
`pub(crate) fn broadcast`, `fn internal`, and `impl Default for AgentRoster`. `connection_service.rs` holds
`mod attachment_progress; pub(crate) use attachment_progress::*;`. The fake's references are positions read off the
fixture text, the way `references_to` does it today.
**Trade-off**: exact and fast; it proves nothing about the server's symbol kinds (Option 2 does).

#### Option 2 (chosen, thin): `packages/tddy-code-restructuring/tests/move_to_crate_widening_acceptance.rs`
A live rust-analyzer and a real toolchain, through `resolution_of`, `applying_the_plan_with` and
`checking_the_plan_with` with `a_sink_that_keeps_what_it_hears`. Registered in `.config/rust-e2e.filterset` and the
`rust-analyzer` group of `.config/nextest.toml`.

#### Option 3 (rejected): compiler-guided widening (a `cargo check` loop parsing E0603/E0616/E0624/E0451)
It can only run after the edit, so `check --deep` could not report anything. It repeats what the reference set already
says, and it is the mechanism that produced the name-based mistakes (decision F1).

### Coverage Requirements

- [ ] Happy: item, struct, field, inherent method, inline-module item and its `mod`, child `mod`, cluster, glob (both facades), escaping type
- [ ] Untouched: unreached, co-moving-only, destination-internal reference, trait-impl member, trait item, enum variant, already `pub`, same names elsewhere, unreached `pub(in …)`
- [ ] Refusal: keyword above the name, nothing written
- [ ] Report: names, reasons, order; console line; journal compatibility; `check --deep` parity with `apply`
- [ ] Actual effects: bytes on disk; `cargo check`; `cargo clippy -D warnings`

## Acceptance tests

Names read as behaviour specifications.
- Library tests 1–18 are **red on `master`** because they do not compile: `DeclarationKind` does not exist,
  `ItemReferences` has no `declared_at`, `resolve_cluster` returns `WorkspaceEdit`, and `VisibilityChange` has no `reason`.
- Live tests 19–23 are red because the moved file keeps `pub(crate)`, so `cargo check` fails with E0603/E0616/E0624, and
  the report is empty.
- Live tests 24–26 are red because `check --deep` prints no `visibility:` line and no glob line.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/move_to_crate_widening.rs` (new; library level, fake `ModuleReferences`, no rust-analyzer)

1. `a_pub_crate_fn_the_origin_still_calls_lands_pub_and_is_reported` — the R2 shape; `` `mint_first_admission_token` pub(crate) -> pub ``.
2. `a_struct_its_fields_and_inherent_methods_the_origin_names_land_pub_and_the_rest_keep_their_visibility` — `AgentRoster`, `AgentRoster::rev`, `AgentRoster::broadcast` widened; `secret` and `internal` byte-identical.
3. `a_declaration_only_a_co_moving_member_reaches_keeps_its_visibility` (a two-member cluster).
4. `a_reference_from_inside_the_destination_crate_widens_nothing`.
5. `a_member_of_a_trait_impl_a_trait_item_and_an_enum_variant_are_never_given_pub` (reached ones, too).
6. `a_same_named_parameter_field_and_function_elsewhere_are_byte_identical` — a `rev` parameter in `runtime.rs`, a `rev` field of another struct in the moved file, and a `broadcast` fn in another module.
7. `an_item_of_an_inline_module_and_the_mod_declarations_on_its_path_land_pub` — reported as `inner::Probe`.
8. `a_child_mod_declaration_the_origin_names_lands_pub_mod` (`mod pty_handle;` → `pub mod pty_handle;`, the #carve R8 shape).
9. `every_non_private_item_of_a_module_its_parent_re_exports_by_glob_lands_pub_with_the_glob_as_its_reason` (`reexport: none`).
10. `a_glob_re_exported_module_moved_behind_a_facade_widens_the_same_items` (`reexport: glob`).
11. `a_private_item_the_parent_glob_cannot_see_keeps_its_visibility`.
12. `a_type_a_widened_signature_names_lands_pub_with_the_signature_as_its_reason` — includes the fixpoint: a widened struct's widened field type.
13. `a_reached_declaration_written_pub_super_or_pub_in_lands_pub_and_an_unreached_one_is_byte_identical`.
14. `a_declaration_already_pub_is_neither_edited_nor_reported`.
15. `a_reached_declaration_whose_keyword_is_on_the_line_above_its_name_is_refused_naming_it_and_nothing_is_written` — `SeamRefused`, exact message of rule 5.
16. `the_widenings_of_a_cluster_merge_into_each_members_header_change_with_one_change_per_file`.
17. `a_widening_with_a_reason_is_stated_with_it_and_one_without_is_stated_as_before` (`console::widening`).
18. `a_journal_record_written_before_reasons_existed_still_reads` (serde of a `VisibilityChange` without `reason`).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/move_to_crate_widening_acceptance.rs` (new live binary; `.config/rust-e2e.filterset` + `rust-analyzer` group)

19. `a_module_whose_pub_crate_items_fields_and_methods_the_origin_uses_moves_behind_a_facade_and_compiles` (the #carve R3 shape; exact moved text; `assert_compiles`).
20. `a_nested_module_its_parent_re_exports_by_glob_moves_and_every_sibling_still_compiles` (the 09-25 shape, including a `pub(crate) trait` that a sibling uses only through method-call syntax).
21. `a_cluster_move_widens_what_the_origin_reaches_of_every_member_and_compiles`.
22. `a_moved_fn_returning_a_type_no_path_names_leaves_the_workspace_clean_under_clippy_with_warnings_denied` (`assert_lints_clean`).
23. `the_server_reports_fields_and_inherent_methods_at_the_positions_the_edits_address` — `resolution_of(…).report` equals the exact list, in order.
24. `a_deep_check_prints_every_widening_the_apply_then_makes_and_writes_nothing` — the sink's `visibility:` lines from the check equal the apply's; the tree is unchanged after the check.
25. `a_deep_check_explains_a_module_with_reached_items_and_no_callers_by_its_parents_glob` — the exact survey line of rule 7.
26. `a_deep_check_of_a_move_item_plan_now_prints_its_widenings_and_reports_the_same_findings` — the side-effect pin, over `tests/same_crate/mod.rs` fixtures.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (brief, 2026-10-09, quoted): "All 12 wave-1 PRDs approved. Every node's own F-decisions: take the
agent's recommendation unless overridden below." And: "Node 7 (`move-widen`) does NOT respell `pub(in …)`; it only
widens a declaration (to `pub`) when code outside the moved files reaches it. Node 7's F4 is overridden."

**Taken** (the recommendation stands, per the developer):
- **F1 — how reach is found.** The language server's references (engine-informed), not a compiler-guided loop. The apply's compile gate stays as the backstop.
- **F2 — glob-visible items.** Every non-private item of a glob-re-exported module, not only the referenced ones (rule 2).
- **F3 — escaping types.** Widened (rule 3), so a `-D warnings` gate stays green.
- **F4 — `pub(in …)`.** **Overridden**: not respelled here. `feature/reshape/move-grouped-use` owns it; this node widens a reached one to `pub`.
- **F5 — report names.** `Type::member`, `inner::Item`.
- **F6 — which members are surveyed.** Every field and member in the moving files, one `textDocument/references` each. The cost is measured in M6, not bounded by a budget.

Decisions taken by this plan:
- **D1**: the trait method keeps its signature, and its contract widens to every declaration. One survey pass serves both
  the caller re-point and the widening, rather than a second trait method asking about items twice.
- **D2**: `reason` is a typed `Option<String>` on `VisibilityChange` rather than a note, so the daemon's renderer states
  it with no edit.
- **D3**: `resolve_cluster` keeps its name and becomes `Resolution`-returning. Its old body is renamed rather than grown,
  because it is on the function-size list.

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

(empty; populated by `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, and M6's cost measurement)

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-move-widen-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-move-widen.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review. It deletes the three ✅ todos and the initial discovery, and narrows the 09-09 entry (item 3 closed; items 2 and 4 marked closed only if their nodes have landed)
- [ ] USER REVIEW — work complete, decide next steps

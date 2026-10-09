# Changeset: cross-crate moves split grouped `use` lines, see co-movers through glob facades, and `check` reads every path

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (a refusal removed) and bug fix (check/apply parity, caller splice, `pub(in …)` read as an edge)
**Stack**: `#reshape` 8/19, branch `feature/reshape/move-grouped-use`, wave 1. PR title:
`feat(code-restructuring): crate moves split grouped uses instead of refusing them (#reshape 8/19)`.
Base in the linear stack: `feature/reshape/move-widen` (K=7). **Real edges**: none in either direction. The base is
textual only (nodes 1–12 share `crate_move/*` and `backends/rust/*`).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-move-grouped-use-initial-discovery.md)
(Exploration 1: whole-work backlog; Exploration 2: this node, E2.1–E2.7).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rn 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds one record, whose value is `none`:
**no 🚧 claimed issue is in the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left.md](../todo/2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left.md) | ✅ **RESOLVED HERE** | Rule P / Rule S over the moved file's `use` statements; `one_use_per_path` deleted. Closed by acceptance tests 1–9, 24. Deleted at wrap |
| [2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move.md](../todo/2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move.md) | ✅ **RESOLVED HERE** | Every engine cause it records: grouped `use` (1–6), glob facade to a co-mover (7, 19, 24), `pub(in crate::origin_module)` as an edge (10–12, 20). Its two non-engine leftovers move out at wrap before deletion: the R6 "third hand edit" (third-crate re-export in the manifest) into the new todo `2026-10-09-restructure-move-to-crate-names-a-re-exporting-crate-instead-of-the-defining-one.md`; the R8/R9 plan corrections (`hooks_and_urls`, `family_proto_bridge`) are history and go into the change-history entry |
| [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md) | ✅ **RESOLVED HERE** | The finding reads `origin_paths`; `header_origin_paths` and `TODO(check-parity-header)` deleted; its own tests and wording (17–21). Deleted at wrap |
| [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) | partial (item 4; claimed by `feature/reshape/move-widen`) | Item 4 (caller re-point spliced inside a grouped `use`) fixed (13–15). At wrap this node strikes item 4 in place; the claimant narrows the rest |
| Function-size list (whole-work discovery Exploration 3): `crate_move/cluster/stranded.rs` `stranded_siblings` (70), `crate_move/cluster.rs` `resolve_cluster` (111) | ⚠ **DURING** | Both are edited and neither may grow (brief, binding). New logic goes in new functions; `resolve_cluster`'s per-member body moves into one helper, so it shrinks |
| [`oversized-file-test-binary.md`](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md), [2026-09-19-test-binary-rs-is-950-production-lines.md](../todo/2026-09-19-test-binary-rs-is-950-production-lines.md) | — Unrelated (textual collision) | `test_binary.rs` is not edited. `header.rs` keeps importing `test_binary::segment_length`; whichever of this node and `feature/reshape/oversized-files` lands second fixes the import path |
| [2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md) | — Unrelated | Test-binary reader, `feature/reshape/apply-robust`'s claim. Its grouped-origin-path refusal (`test_binary.rs:801-822`) is deferred to a new todo |
| [2026-10-04-restructure-reparent-module-first-cut-limits.md](../todo/2026-10-04-restructure-reparent-module-first-cut-limits.md), [2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md](../todo/2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md) | — Unrelated | Their grouped-`use` refusals are same-crate (`reparent_module`, `item_move/sites.rs:335`); not widened here |
| [2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses.md](../todo/2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses.md) | — Reference | The `pub(in …)` respelling writes `pub(crate)`, never `pub`, so this node adds no debt to it |
| Other `packages/tddy-code-restructuring/docs/code-issues/*` | — | Not in the path |

## Affected Packages

- **`tddy-code-restructuring`** ([README.md](../../../packages/tddy-code-restructuring/README.md)):
  - new `src/crate_move/use_group.rs` (+ `use_group/refusals.rs`): the group rule, relocated **down** from
    `src/backends/rust/repoint_facade/group.rs`, plus `members_of` (from `item_move/sites.rs`), `split_use` and `use_statements`
    (from `item_move/text.rs`), and the statement-level wrapper (`group_rewrite`, `attribute_above`, `indentation` from
    `repoint_facade.rs`). Engine-driven `move_item` (F1);
  - `src/crate_move.rs` (`mod use_group`), `src/crate_move/header.rs` (`rewrite_of`, `reach`, `Header`),
    `src/crate_move/survey.rs` (`SurveyedPath.restriction`), `src/crate_move/source_scan/sighting_walk.rs`
    (`Sighting.in_visibility`), `src/crate_move/moving.rs` (`caller_changes`), `src/crate_move/cluster.rs` (`resolve_cluster`
    collects callers across members), `src/crate_move/cluster/stranded.rs` (`paths_naming_the_origin`, remedy wording);
  - `src/backends/rust/repoint_facade.rs` and `repoint_facade/{rewrite,refusals}.rs` (import from `crate_move::use_group`;
    `group.rs` removed), `src/backends/rust/item_move/{text,sites,imports}.rs` (import the moved helpers).
  - Docs at wrap: [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md) (group rule, glob facade,
    `pub(in …)`), [repoint-facade.md](../../../packages/tddy-code-restructuring/docs/repoint-facade.md) (where `group.rs` went;
    nested flattening), [facades.md](../../../packages/tddy-code-restructuring/docs/facades.md) if it names the refusal;
    [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) (`## Path survey`, `### repoint_facade_imports`,
    `## Known limitations`); [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) only if it
    tells authors to split groups by hand.
- **`tddy-tools`, `tddy-index-daemon`**: no source change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-move-grouped-use.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-move-grouped-use.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Path survey`, `## Known limitations`, `### repoint_facade_imports`

## Summary

`move_module_to_crate` and `move_cluster_to_crate` rewrite a grouped `use` whose leaves need different qualifiers by the rule
`repoint_facade_imports` already uses, instead of refusing it: Rule P in place, otherwise Rule S. Rule S is extended to flatten
a nested member whose leaves disagree. The same rule rewrites a caller's grouped `use` (`reexport: none`).

A path that reaches a co-moving member through an origin glob facade counts as co-moving. A `pub(in crate::<module>)`
restriction is a visibility: `pub(crate)` when the module stays, `pub(in crate::<landing>)` when it moves. Static `check`'s
stranded-sibling finding reads every path `apply`'s cycle refusal reads.

## Background

`#carve` 21/21 (PR #536) moved three clusters out of `tddy-session-lifecycle`. Each was refused by `check --deep`, and each was
unblocked by developer-consented hand edits made before the engine ran (todo `2026-10-08-hand-split-…`):
- 19 grouped `use` lines split;
- every path through `connection_service`'s glob facades re-spelled;
- 22 `pub(in crate::connection_service)` widened to `pub`.

The engine already splits groups for another operation (`repoint_facade/group.rs`). It already follows glob facades
(`reexports::followed`) and tests the followed path for the body finding. It simply did not apply either answer in the
move's header pass.

Two older defects live on the same code. `check`'s stranded-sibling finding reads only the top-level header, and a test pins the
gap. `reexport: none` caller re-points can be spliced inside someone else's group.

## Responsibility

- One shared, text-only group rule in `crate_move::use_group`, consumed by the move's header pass, the move's caller pass and
  `repoint_facade_imports`. Rule P output stays byte-identical to today. Rule S splits; a nested member whose leaves disagree is
  flattened. An attribute or doc comment above a statement that would split is refused.
- `reach` tests co-movement on `resolved` and then on `defined_at`.
- The survey marks a path inside `pub(in …)` as a restriction. The header pass respells it — `pub(crate)` or
  `pub(in crate::<landing>…)` — and never counts it as an edge or a manifest crate. **This node owns every `pub(in …)`
  respelling in crate moves** (developer decision 2026-10-09). `feature/reshape/move-widen` only widens declarations reached
  from outside the moved files.
- Caller re-points are collected across a cluster's members and written once per `use` statement through the group rule.
- The stranded-sibling finding reads `origin_paths`. For a crate-root item it gives a remedy that does not name
  `move_cluster_to_crate`.
- Three pinned tests are rewritten to the new behaviour, never deleted. Consent was given for two (2026-10-09); the third is F6.

## Rules (the contract)

**R1 — Which leaves form a statement.** The survey's leaves of one `use` tree share a `site` (`header.rs:87-91`). The
statement span is the `use_statements` range (masked text) that contains the site. A body path is never part of a statement
and keeps today's span edit.

**R2 — Leaves.** For each surveyed leaf, `Leaf { written, rewritten }`. `rewritten` is the `Reach.rewritten` of
`header.rs:139-171`, or `written` when `Reach.rewritten` is `None` (a `self::`/`super::` inside the moved module). A leaf whose
`rewritten == written` is a **kept** member under Rule S.

**R3 — Rule P.** If every leaf agrees on what the statement's common prefix becomes, only the prefix span is replaced. This is
today's edit, byte for byte, including `keeps_its_name` (`use crate::roster;` → `use crate::records as roster;`).

**R4 — Rule S.** Otherwise kept members stay under the old prefix, first. Each lifted member follows as
`<visibility> use <path>;` in member order, each on its own line with the statement's indentation. A lifted leaf whose last
segment changes keeps its name with `as <old>`; a member's own alias is kept; a glob member is lifted as `<path>::*`. A statement
with no kept member becomes only its lifted statements.
- R6 input: `use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};`
- R6 output: `use crate::seed_codebase;\nuse crate::seeded_clone_guard;\nuse crate::seed_codebase::SeededAgentClones;`

**R5 — Nested members.** A nested member (`a::{x, y}`) whose leaves share one new prefix is lifted whole (today). One whose
leaves do not is **flattened**: one statement per leaf, in leaf order, under the same name rules. This replaces
`nested_member_reaches_two_crates` for both callers of the rule (F3, approved).

**R6 — Refusals kept.** A split statement with an attribute or doc comment on the line above is refused with
`attribute_above_a_split` (`<file>:<line>: … write one \`use\` per path`), naming the file and line; nothing is written. Also
refused: a statement the rule cannot read (`unreadable_use`), and a path spelled across whitespace or comments (`header.rs:187-191`,
unchanged).

**R7 — Co-movement through a glob facade.** `reach` first tests `travels_with(resolved − origin::)` (today). On no match it tests
`travels_with(defined_at − origin::)`. On a match there, the landing is `crate::<member's last segment><rest of defined_at after
the member>`, with `names: None, edge_back: false`. Everything else is unchanged.

**R8 — `pub(in …)`.** `Sighting.in_visibility` is set for a path whose preceding tokens are `pub` `(` `in`. Such a path becomes
`SurveyedPath.restriction = true`. In the header pass a restriction:
- whose resolved path reaches a co-moving member (`travels_with`) is rewritten to `crate::<landing>…`, as today;
- otherwise is replaced, together with its `pub(in …)` wrapper, by `pub(crate)`;
- is never pushed to `crates_named`, `dev_crates_named` or `origin_paths`.

`stays_behind_through_a_body` ignores restrictions as well (today it already does by shape; pinned by test 20).

**R9 — Callers.** `resolve_cluster` collects every member's `PlannedRewrite`s (with `reexport: none`) before writing caller
changes. `caller_changes` groups a file's rewrites by containing `use` statement:
- a rewrite in no statement keeps today's span edit;
- the rewrites in one statement become leaves `{ written: <group prefix chain> + <written path>, rewritten: to }`, other
  members are kept, and one statement edit is written through R3–R6.

So `use crate::{connection_service::agent_roster, livekit_rooms_stream::RoomRoster, spawn_worker};` becomes
`use crate::{connection_service::agent_roster, spawn_worker};\nuse dest::livekit_rooms_stream::RoomRoster;`, and
`use crate::{a::X, b::Y};` (both members moving) becomes `use dest::{a::X, b::Y};`.

**R10 — Stranded finding.** `paths_naming_the_origin` returns `header.origin_paths`. The finding's wording is unchanged for a
path inside a module. When every listed path is an item of the origin's crate root, the remedy reads: "…cut its dependency on
`<item>` before moving it" — no `move_cluster_to_crate`. A mixed list keeps the cluster remedy and names the root items
separately.

## Boundaries

- **No other operation's behaviour changes**, with one exception: `repoint_facade_imports` now flattens a nested member whose
  leaves disagree instead of refusing it (R5). Same-crate moves, `move_test_binary_to_crate` and `retarget_impl` are untouched.
- **Rule P edits are byte-identical**; a move that never met the refusal writes the same text (pinned by test 2 and every existing
  `move_paths_acceptance` / `cluster_move_acceptance` expectation).
- **No widening.** A `pub(in …)` becomes `pub(crate)`, never `pub`. Widening a declaration that code outside the moved files reaches
  belongs to `feature/reshape/move-widen`.
- **No change to `reexports::followed` or the survey's resolution**, only a flag on a sighting. A path through a third crate's
  re-export is not re-pointed (new todo).
- **No new refusal.** The dependency-cycle refusal's wording is unchanged.
- **Moves are engine-driven.** The relocation of the group rule into `crate_move` is done with `tddy-tools restructure`
  (`move_item`). Hand edits are only build fixes after an engine move, each recorded with a `TODO(reshape-8-hand-fix)` marker and
  listed in a todo entry. An engine refusal stops the work and is put to the developer.
- **No function on the size list grows.** `resolve_cluster` and `stranded_siblings` end at or below their current length.
- **No new live binary registration.** Test 24 joins `cluster_move_acceptance`, which is in `.config/rust-e2e.filterset:46`.
  Its `rust-analyzer` nextest-group entry is node 1's (brief).

## Dependencies

This node has no parent: it consumes no behaviour from any other `#reshape` node. Its base, `feature/reshape/move-widen`, is
textual only (shared `crate_move/*` files).

## Draft PR contract

Published with the wave-2 contract commit — the first push of this PR, not its deliverable. **Owned surface, new today:**

- `crate_move::use_group` (`pub(crate) mod`):
  - `pub(crate) struct Leaf { pub(crate) written: String, pub(crate) rewritten: String }`
  - `pub(crate) fn split_or_reprefix(statement: &str, leaves: &[Leaf]) -> Result<String>` (R3–R5; moved, then widened to
    `Leaf`)
  - `pub(crate) fn statement_edit(file: &str, text: &str, span: Range<usize>, leaves: &[Leaf]) -> Result<TextEdit>` (R6
    attribute refusal and re-indentation)
  - `pub(crate) fn statement_containing(text: &str, at: usize) -> Option<Range<usize>>`
  - `pub(crate) fn members_of(inner: &str) -> Vec<&str>`, `pub(crate) fn split_use(statement: &str) -> Option<(&str, &str)>`,
    `pub(crate) fn use_statements(masked: &str) -> Vec<Range<usize>>` (moved, visibility widened from `pub(in crate::backends::rust)`)
  - `use_group::refusals::{attribute_above_a_split, unreadable_use}` (moved from `repoint_facade/refusals.rs`)
- `source_scan::Sighting.in_visibility: bool`; `survey::SurveyedPath.restriction: bool`.
- `header::Header` loses `header_origin_paths`; `header::one_use_per_path` is deleted.
- `moving::caller_changes(workspace: &Workspace<'_>, rewrites: Vec<PlannedRewrite>) -> Result<Vec<FileEdit>>` keeps its
  signature; its contract becomes R9.
- No public (`lib.rs`) export changes; no plan field, CLI flag or wire change.
- Failing tests: 1, 3–7, 9, 10, 12–15, 17–19, 21–24 below. Tests 2, 8, 11, 20 are green pins; 16 is a probe (F5).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes — on `master` alone.
**Concurrent with:** every other wave-1 node (`widen-same-crate`, `multi-seam-extract`, `tidy-facades`, `extract-method-clean`,
`move-children`, `methods-leave-type`, `move-widen`, `new-crate`, `apply-robust`, `move-item-paths`, `anchors-outline`); no edge
between them. Expect textual conflicts in `crate_move/header.rs` with `feature/reshape/move-widen`,
`feature/reshape/move-children` and `feature/reshape/move-item-paths`, and in `cluster.rs` with `feature/reshape/move-children`.
**Blocks:** none.
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`,
`4→19`, `17→19`. None touches node 8.

## Successor PRs

None in this stack: no node consumes this one's behaviour. (The later engine-driven crate split, stack 2, is a consumer, but it
is not part of `#reshape`.)

## Scope

- [ ] **Relocation**: the group rule and its helpers into `crate_move::use_group` by engine `move_item`; `repoint_facade` green on
  its existing tests (except the F6 test)
- [ ] **Nested flattening** (R5)
- [ ] **Header pass**: statement edits through the rule (R1–R4, R6); `one_use_per_path` deleted
- [ ] **Glob facade** co-movement (R7)
- [ ] **`pub(in …)`** flag and respelling (R8)
- [ ] **Callers** collected across the cluster, statement edits (R9)
- [ ] **Stranded finding** reads `origin_paths`; root-item remedy; `header_origin_paths` and the TODO marker deleted (R10)
- [ ] **Pinned tests** rewritten (21, 22, 23)
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, clippy `-D warnings`, `cargo fmt`; no file past 500
  production lines; `resolve_cluster` / `stranded_siblings` not grown

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `header.rs:177-222` `rewrite_of` writes one prefix replacement per `use` tree. It refuses with `one_use_per_path`
  (`header.rs:238-244`) at `:198-200` and `:205-206`; the refusal reaches `apply` (`cluster.rs:129`) and static `check`
  (`stranded.rs:141`).
- `repoint_facade/group.rs:20-78` holds Rule P / Rule S. It depends on `item_move::{sites::members_of, text::split_use}`
  (`pub(in crate::backends::rust)`), and nested disagreement is refused at `group.rs:181-182`. `crate_move` has no dependency on
  `backends`.
- `header.rs:142-144` tests co-movement on `resolved` only. `preconditions.rs:126-135` (body finding) tests `defined_at`.
- `sighting_walk.rs:107-121` reads `pub(in crate::x)` as a body path; `header.rs:166-170` makes it an edge back. If not refused,
  it would be written as `pub(in origin::x)`.
- `crate_move.rs:233-254` + `header.rs:344-364` re-point a caller's path span in place, inside any group. `cluster.rs:126-145`
  builds caller changes per member; `MergedChanges::add` (`cluster.rs:227-232`) concatenates them.
- `header.rs:27-37,106-110` `header_origin_paths`; `stranded.rs:132-143` reads it. `check_precondition_parity.rs:497`
  pins the resulting gap.

### State B (Target)

A cluster with lifecycle's import shape passes `check --deep` and applies with no pre-move hand edit:
- grouped `use` lines are split or re-prefixed;
- facade paths to members land at the member;
- `pub(in …)` restrictions are respelled.

`check` reports exactly the stranded paths `apply` refuses on. Callers' groups always resolve.

### Delta (What's Changing)

#### `tddy-code-restructuring`

- **New** `crate_move/use_group.rs` (~230: moved `group.rs` ~170 + helpers ~60) and `crate_move/use_group/refusals.rs` (~40).
  `repoint_facade/group.rs` removed.
- **`header.rs`**: `rewrite_of` builds `Leaf`s and calls `use_group::statement_edit` for a `use` tree. `reach` gains the
  `defined_at` test (R7) and the restriction branch (R8). `Header.header_origin_paths`, `header_lines` and `one_use_per_path`
  are deleted. The file stays under 500.
- **`survey.rs`**, **`sighting_walk.rs`**: one flag each.
- **`moving.rs`**: `caller_changes` groups by statement (new helper `statement_rewrites`).
- **`cluster.rs`**: `resolve_cluster`'s per-member body moves into `fn absorb_member(...)`; caller rewrites are accumulated and
  written once after the loop.
- **`stranded.rs`**: `paths_naming_the_origin` → `origin_paths`; new `fn remedy_for(...)` (R10).
- **`repoint_facade.rs`**, **`repoint_facade/{rewrite,refusals}.rs`**, **`item_move/{text,sites,imports}.rs`**: imports from
  `crate_move::use_group`. `Rewrite` maps to `Leaf` at the one call site.

## Implementation milestones

- [ ] **M1** Relocation by engine `move_item` (plan in a Refactor changeset under the `code-restructuring` skill). Build fixes only;
  each fix gets `TODO(reshape-8-hand-fix)` and a line in todo `2026-10-09-reshape-8-hand-fixes-after-the-group-rule-move.md`
  (written only if any fix was needed). Existing `repoint_facade` tests green.
- [ ] **M2** `Leaf` and nested flattening (R5); tests 5, 23.
- [ ] **M3** Header statement edits (R1–R4, R6); tests 1–4, 6, 9, 22.
- [ ] **M4** Glob facade (R7); tests 7, 8, 16 (probe), 19.
- [ ] **M5** `pub(in …)` (R8); tests 10–12, 20.
- [ ] **M6** Callers (R9); tests 13–15.
- [ ] **M7** Stranded finding (R10); tests 17, 18, 21.
- [ ] **M8** Live R6 test 24; docs staged; scoped gate; length and function-size checks.

## Testing plan

### Testing Strategy

**Primary: library level, no rust-analyzer.** `resolve_cluster` is public, and so is `crate_move::ModuleReferences`, so a new test
binary drives whole cluster moves over a fake reference set. It asserts on edits applied to text. The static findings go through
`siblings_left_behind` / `unrunnable_moves`. One live test proves that the R6 shape compiles after `check --deep` and `apply`.

#### Option 1 (chosen): `tests/grouped_use_crate_move.rs` (new, library level)

Fixture: workspace `origin` → `destination` (+ `shared`). `origin/src/lib.rs` declares `pub mod connection_service;`, and
`connection_service.rs` declares `pub mod seed_codebase; pub mod seeded_clone_guard; pub mod agent_host_callbacks; pub mod roster;`
with `pub use seed_codebase::*;` and `pub use roster::*;`, plus a staying `pub mod host;`. A fake `ModuleReferences`, modelled on
`AKnownReferenceSet` (`src/crate_move/cluster.rs:345-396`), stands in for rust-analyzer, and an `applied` helper
(`cluster.rs:423-438` pattern) shows the resulting text. Not in the e2e filterset.
**Trade-off**: exact on text, fast. It does not prove compilation; test 24 does.

#### Option 2 (chosen, thin): one live test in `tests/cluster_move_acceptance.rs`

`assert_compiles` after `check --deep` and `performing`.

#### Option 3 (rejected): unit tests inside `cluster.rs`

That file is 921 lines with tests and on the function-size list's path. New behaviour belongs in an integration binary.

### Coverage Requirements

- [ ] Rule P unchanged; Rule S; names and aliases; visibility and indentation; nested flattening; attribute refusal
- [ ] Glob facade to a member (co-moving) and to a module that stays (edge)
- [ ] `pub(in …)`: staying module, co-moving module, never an edge or a manifest line, never written with a crate name
- [ ] Callers: split, one edit per statement across members, Rule P on a fully moving group
- [ ] Static parity: mixed group, nested `use`, glob facade, restriction, crate-root body path
- [ ] Actual effect: the R6 shape compiles

## Acceptance tests

Names read as behaviour specifications. Every test is **red on `master`** unless marked; each red reason is given.

### `packages/tddy-code-restructuring/tests/grouped_use_crate_move.rs` (new; library level over `resolve_cluster`, no rust-analyzer)

1. `a_moved_files_group_whose_leaves_need_different_qualifiers_is_split_kept_members_first` — red: `one_use_per_path`.
2. `a_moved_files_group_whose_leaves_agree_is_re_prefixed_in_place_as_before` — **green pin** (Rule P bytes, incl. `as roster`).
3. `a_lifted_member_keeps_the_name_it_bound_and_its_own_alias` — red: refused today.
4. `a_split_keeps_the_visibility_and_the_indentation_of_the_statement_it_replaced` (`pub use` group; a group in an inline
   non-test `mod`) — red: refused today.
5. `a_nested_member_whose_leaves_need_two_prefixes_is_flattened_into_one_use_per_leaf` — red: refused today.
6. `a_group_that_must_split_under_an_attribute_or_doc_comment_is_refused_naming_the_file_and_line_and_nothing_is_written` — red:
   today's refusal is `one_use_per_path`, which names neither the attribute nor the line.
7. `a_path_through_an_in_crate_glob_facade_to_a_co_moving_member_lands_at_that_member_and_is_no_edge` — the R6 file, exact
   three-line output, no `origin` in the destination manifest. Red: refused as a group, and `SeededAgentClones` reads as an edge.
8. `a_path_through_an_in_crate_glob_facade_to_a_module_staying_behind_is_still_an_edge_back` — **green pin** (cycle refusal
   names the followed path).
9. `a_split_group_whose_kept_member_stays_in_the_origin_is_refused_as_a_dependency_cycle_not_as_a_group` — red: today's message
   is "write one `use` per path".
10. `a_pub_in_restriction_naming_a_module_that_stays_becomes_pub_crate_and_is_no_edge` — red: cycle refusal on
    `origin::connection_service`.
11. `a_pub_in_restriction_naming_a_co_moving_module_is_rewritten_to_where_it_lands` — **green pin** (the `travels_with` branch).
12. `a_pub_in_restriction_adds_no_manifest_line_and_is_never_written_with_a_crate_name` (`reexport: none`, no caller) — red:
    writes `pub(in origin::connection_service)` and adds `origin` to the destination manifest.
13. `a_callers_re_point_inside_a_grouped_use_splits_the_group_instead_of_splicing_into_it` (`reexport: none`) — red: writes
    `use crate::{…, destination::livekit_rooms_stream::RoomRoster, …}`.
14. `a_callers_group_naming_two_members_of_one_cluster_receives_one_statement_edit` — red: two span edits, both inside the group.
15. `a_callers_group_whose_every_leaf_moves_is_re_prefixed_in_place` — red: each leaf is spliced separately.
16. `a_path_through_a_glob_the_origin_takes_from_another_crate_is_no_edge` (`pub(crate) use shared::progress::*;`, the R8
    `AttachmentProgressSink` shape) — **probe (F5)**: if red, it is fixed here; if green, it is kept as a pin and the PR body says
    the R8 report did not reproduce.

### `packages/tddy-code-restructuring/tests/cluster_move.rs` (existing; static `siblings_left_behind`)

17. `a_static_check_of_a_cluster_with_a_mixed_grouped_use_reports_nothing_and_does_not_refuse` — red: `stranded_by` panics on
    `one_use_per_path`.
18. `reports_a_sibling_reached_from_a_use_nested_in_a_function` — red: header-only reading.
19. `reports_nothing_for_a_cluster_whose_member_reaches_another_through_a_glob_facade` — red: "stays behind".
20. `does_not_report_a_pub_in_restriction_as_a_stranded_sibling` — **green pin** on `master` (the header-only reading never sees a
    body path). It guards R8 once test 18 widens the reading to every path: without R8 it goes red.

### Rewritten pinned tests (consent: 21 and 22 given 2026-10-09; 23 pending, F6)

21. `packages/tddy-code-restructuring/tests/check_precondition_parity.rs:497` `a_body_path_to_an_item_at_the_crate_root_is_no_finding`
    → `a_body_path_to_an_item_at_the_crate_root_is_reported_as_apply_refuses_it_without_suggesting_a_cluster` — red: no finding
    today (while `apply` refuses: `move_paths_acceptance.rs:206`).
22. `packages/tddy-code-restructuring/tests/move_paths_acceptance.rs:244` `a_use_group_whose_members_land_in_different_crates_is_refused_with_the_fix`
    → `a_use_group_whose_kept_member_stays_in_the_origin_is_split_and_then_refused_as_a_cycle` (live, existing filterset entry) —
    red: today's message is the group refusal.
23. `packages/tddy-code-restructuring/tests/repoint_facade_imports_acceptance.rs:421` `a_nested_group_member_is_lifted_whole_when_its_leaves_agree_and_refused_when_they_do_not`
    → `…_and_flattened_into_one_use_per_leaf_when_they_do_not`. The `mix::{Limits, Roster}` tree becomes
    `use crate::{b::Thing};\nuse kernel::config::Limits;\nuse agents::roster::Roster;`. Red: refused today.

### `packages/tddy-code-restructuring/tests/cluster_move_acceptance.rs` (existing live binary, `.config/rust-e2e.filterset:46`)

24. `moves_a_cluster_whose_members_name_each_other_through_grouped_uses_and_a_glob_facade_and_the_workspace_compiles` — the R6 shape
    (three members, two grouped `use`s, one through `pub use seed_codebase::*`, one `pub(in crate::connection_service)` fn):
    `check --deep` has no findings, `apply` succeeds, `assert_compiles`. Red: refused before any server answers.

The three unit tests in today's `group.rs` move with it unchanged (green).

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

**Taken by the developer (2026-10-09, PRD review):**
- **F1** — the group rule is relocated **down** into `crate_move` by engine `move_item`; every hand fix after the move gets a TODO marker
  and a todo entry.
- **F2** — a `pub(in …)` naming a module that stays becomes `pub(crate)`. This node owns all `pub(in …)` respelling in crate moves;
  `move-widen` only widens reached declarations.
- **F3** — a nested member whose leaves disagree is flattened, in both operations.
- **F4** — consent to rewrite `check_precondition_parity.rs:497` and `move_paths_acceptance.rs:244`.
- **F5** — a probe test for the cross-crate glob (`AttachmentProgressSink`) shape.

**OPEN** (each with a recommendation):
- **F6 — a third pinned test.** `repoint_facade_imports_acceptance.rs:421` asserts the nested refusal that F3 removes. The PRD said
  no test pinned it; that was wrong, because the test asserts the member name rather than the refusal text. (a) **Rewrite it to the
  flattened output** — *recommended*, the direct consequence of F3. (b) Keep the refusal in `repoint_facade_imports` only (a
  `flatten: bool` on the shared rule), at the cost of two behaviours for one shape.
- **F7 — where the root-item remedy applies.** (a) **Per listed path: root items named separately with "cut the dependency", module
  paths keep the cluster remedy** — *recommended*. (b) One message for all, dropping the cluster suggestion whenever any path is a root
  item.
- **F8 — the split's shape.** (a) **One statement per lifted member, kept members first** (Rule S as is) — *recommended*: consistent with
  `repoint_facade_imports`, and rustfmt plus the tidy collapse leftovers. (b) Re-group lifted members by shared new prefix
  (`use crate::{seed_codebase, seeded_clone_guard, seed_codebase::SeededAgentClones};`): fewer lines, a second rule.

Decisions taken by this plan: no new refusal; Rule P bytes unchanged; restrictions are never edges; caller statements are written
once per cluster.

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

- [x] Record initial discovery (`2026-10-09-reshape-move-grouped-use-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-move-grouped-use.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (incl. F6)
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
- [ ] Wrap documentation (/wrap-context-docs) — deletes the three claimed todos and this node's initial discovery; strikes item 4 of the
  09-09 entry; moves the hand-split file's R6 third-edit item into its new todo first
- [ ] USER REVIEW — work complete, decide next steps

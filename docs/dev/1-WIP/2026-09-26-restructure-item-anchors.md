# Changeset: Restructure item anchors — an anchor is an LSP path, a line/col is only a hint

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-item-anchors-initial-discovery.md).

State A below is distilled from that file.

## Stack

`#live-plan` 1/7 — branch `feature/live-plan/item-anchors`, base `master`.
PR: [#537](https://github.com/uppin/tddy-coder/pull/537)

## Responsibility

- The `item` and `items` anchor kinds, their JSON shape, parse and validation.
- The item-path resolver in the Rust backend: `file` → crate + module path → outline walk → exact
  range; the fingerprint check; every refusal it can produce.
- Resolving item anchors **at run open** into the coordinates the ledger already translates.
- The schema v2 header (`files.<path>.{sha256, modified}` hints) and v1 compatibility.
- `restructure anchors --items` (working, emitting `items`) and `anchors --at` (new, emitting `item`),
  on the in-process path and through the daemon's `Anchors` RPC.
- The refusal of an item anchor in a file no backend can resolve items for (`NoBackend`,
  `UnsupportedOp`); the TypeScript sidecar is outside this crate.

## Boundaries

- Does **not** hold plans in memory, assign op ids, or write a plan back to disk — the plan is still
  read per run.
- Does **not** refresh anchors after an operation beyond what the ledger already does within a run.
- Does **not** change any operation's semantics, nor the ledger/journal formats.
- Does **not** convert existing plans; v1 plans are untouched.

## Dependencies

None — this is the stack's root, based on `master`.

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `plan.rs`: `Anchor::Item { item, file, start, end, fingerprint, hint }`,
  `Anchor::Items { file, items, fingerprints }`, `ItemPath` (parse/display of
  `crate::m::T::f` and `<T as Trait>::f`), `Fingerprint`, `FileHint`, the v2 header in `Plan`.
- `backends/rust/item_path.rs`: `resolve_item(&mut self, uri, &ItemPath) -> Result<ResolvedItem>`
  (`ResolvedItem { range, text_fingerprint }`), `TODO(item-anchors): implement`.
- `runner`: `resolve_item_anchors(&Plan, &mut dyn Backend) -> Result<Plan>` (run-open resolution).
- `RestructureAnchorsArgs.at`; `AnchorsRequest.at` + `AnchorsResponse.anchor_json` in
  `code_index.proto`.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — every test drives this node's own resolver against a fixture
crate through the real rust-analyzer; nothing asserts a later node's behaviour.
**Concurrent with:** `move-paths`, `move-facades`, `extraction-defects`
**Blocks:** `plan-store` (its per-op refresh re-resolves item anchors through this resolver), and
through it `live-plans`

Real dependency edges, as opposed to the branch line:

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

- `feature/live-plan/plan-store` — holds plans in the daemon and keeps the applied plan's item
  anchors current.

## Prerequisites

Open items this change runs into.

### ⚠ PARTIALLY ADDRESSED HERE (record stays, narrowed) — `restructure anchors` resolves no item — [`broken-restructure-anchors-empty-outline.md`](../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md)

Blocking: the item-path resolver walks the same `documentSymbol` outline that comes back empty. Every
route around it is wrong — a second outline reader would duplicate the defect's cause, and parsing
the file ourselves would bypass the LSP the anchor exists to trust. The empty-outline wait is reproduced and fixed in this node; the warm-path refusal and the cold-path
`lsp server exited` are not shown fixed at repo scale, so the record stays, narrowed, and this node's
wrap does **not** delete it.

### ℹ ANSWERED ELSEWHERE — `restructure snapshot` cannot rebase a stale plan — [`2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md`](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md)

This node makes item anchors survive edits outside their item, which removes most of the need; the
entry itself is claimed by `feature/live-plan/live-plans` (node 3/7, the lowest node that fixes it
end to end).

### ℹ UNRELATED — older WIP changeset in the same packages — [`2026-09-17-restructure-refusal-truth-and-authoring-gates.md`](./2026-09-17-restructure-refusal-truth-and-authoring-gates.md)

Its code landed in #502 and its documents were never wrapped. This node touches the authoring
section of the same skill; it does not edit that changeset.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) — anchor
  kinds, v2 header, item-path resolver, run-open resolution, `anchors --at`
- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) —
  `AnchorsRequest.at`, `AnchorsResponse.anchor_json`
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — `index_client.rs` carries
  `--at`
- **Skill**: `.agents/skills/code-restructuring/SKILL.md`, `references/plan-schema.md` — authoring
  with item anchors

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-restructure-item-anchors.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)
- [Warm code-intelligence daemon](../../ft/coder/warm-code-intelligence-daemon.md)

## Summary

Adds `item`/`items` anchors — a crate-rooted item path plus a range relative to the item, with a
fingerprint of the item's text — resolved exactly through rust-analyzer's outline at run open. The
absolute position becomes an orientation hint; the v2 header's per-file hashes become hints too.

## Background

`find_symbol` returns the first outline node with a matching name; range anchors are trusted
verbatim and corrected only inside one run. One unrelated PR made six #524 plans stale.

## Scope

- [x] `item` / `items` anchors, parse + validation
- [x] Item-path resolver with fingerprint check and refusals
- [x] Run-open resolution into ledger coordinates
- [x] Schema v2 header; v1 unchanged
- [x] `anchors --items` fixed (empty outline) and emitting `items`; `anchors --at` emitting `item`
- [x] Daemon `Anchors` RPC and `tddy-tools` client carry the new shape
- [x] An item anchor in a file no backend can resolve items for is refused by name
- [x] Skill and plan-schema reference updated

## Technical Changes

### State A (Current)

- `Anchor::{Symbol{file,path}, Range{file,start,end}}` (`plan.rs:15`); `Symbol.path` is a bare name.
- `find_symbol` (`backends/rust.rs:2226`) — first match in a depth-first walk of `documentSymbol`.
- `anchor_range` / `rename_symbol` read `Range` verbatim; `PositionLedger::translate` corrects it
  within a run; nothing corrects it across runs.
- Header `{"v":1,"snapshot":{path: sha256}}`; `verify_snapshot` refuses on any drift.
- `places_of` (`rust.rs:1970`) refuses every item — empty outline.
- `AnchorsResponse { SourceRange range }`.

### State B (Target)

- `Anchor` gains `Item` and `Items`; `Symbol`/`Range` remain for v1.
- `ItemPath` parses `crate::a::b::T::f` and `crate::a::<T as Tr>::f`.
- The resolver maps `file` to its crate name (manifest `package.name`, `-`→`_`) and module path
  (`src/lib.rs`/`main.rs` → root, `a/mod.rs` and `a.rs` → `a`), refuses a prefix mismatch, walks
  outline children by segment (modules, types, impl blocks keyed by self type and trait), refuses
  absent / ambiguous segments, hashes the item's full range text and compares the fingerprint.
- At run open every item anchor is resolved to absolute coordinates on the starting tree and handed
  to the ledger as original-snapshot coordinates.
- v2 header `{"v":2,"files":{path:{"sha256","modified"}}}` — reported on drift, never refused.
- `anchors --items` returns an `items` anchor; `anchors --at L:C[-L:C]` the innermost enclosing
  item's `item` anchor; `AnchorsResponse.anchor_json` carries either.

### Delta

#### tddy-code-restructuring
- `plan.rs`: anchor kinds, `ItemPath`, v2 header, validation (range inside item, adjacency).
- `backends/rust/item_path.rs` (new): resolver; `backends/rust.rs`: route item anchors, fix the
  outline read behind `places_of`.
- `runner.rs` / `runner/entry_points.rs`: `open_run_resolving_anchors` — item anchors resolve, then
  the baseline compile gate, then `.restructure/` is written; shared by the CLI apply and the daemon's.
- `restructure_args.rs` / `restructure_cli.rs`: `anchors --at`.
- Registry: an item anchor in a file no backend claims is `NoBackend`; a backend without an item
  resolver is `UnsupportedOp`.

#### tddy-index-daemon
- `proto/code_index.proto`: `AnchorsRequest.at` (optional `SourceRange`), `AnchorsResponse.anchor_json`.
- `queries.rs`: anchors query returns the anchor; `cli.rs`: `--at`.

#### tddy-tools
- `index_client.rs`: pass `--at`.

## Implementation Milestones

- [x] `ItemPath` parse/display round-trips, including trait-qualified members
- [x] Anchor kinds parse; v1 plans parse byte-identically
- [x] Outline read fixed; `anchors --items` returns items on warm and cold paths
- [x] Resolver resolves module items, inherent and trait members, inline-module items
- [x] Fingerprint mismatch / absent / ambiguous / prefix mismatch refusals
- [x] Run-open resolution; an `extract_method` through an item anchor matches the range-anchor edit
- [x] `anchors --at`; daemon RPC carries it
- [x] v2 header drift reported, not refused

## Testing Plan

### Testing Strategy

Acceptance tests at the crate's integration level (`tests/*.rs`) against real rust-analyzer on small
fixture crates from `tests/harness/`, as every other acceptance suite in this crate does — the point
of the feature is what rust-analyzer's outline says, so a double would test nothing. Unit tests in
`plan.rs` for parsing and `ItemPath`, and in `item_path.rs` for the outline walk over recorded
`documentSymbol` JSON.

### Coverage Requirements

Every refusal the resolver can produce has a test naming it; every anchor field has a parse test.

## Acceptance Tests

### tddy-code-restructuring — `tests/item_anchor_acceptance.rs`

- `an_item_anchor_resolves_exactly_after_lines_are_inserted_above_its_item`
- `an_item_anchor_ignores_a_wrong_hint`
- `two_inherent_impls_resolve_their_own_new`
- `a_trait_member_collision_is_refused_until_the_trait_is_named`
- `an_edit_inside_the_anchored_item_is_refused_naming_the_item`
- `an_edit_outside_the_anchored_item_is_not_refused`
- `an_item_absent_from_its_file_is_refused_without_searching_elsewhere`
- `a_module_prefix_that_does_not_match_the_file_is_refused`
- `a_relative_range_outside_its_item_is_refused_as_malformed`
- `extract_method_through_an_item_anchor_matches_the_range_anchor_edit`
- `a_v2_plan_with_unrelated_file_drift_runs`
- `a_v1_plan_with_the_same_drift_is_still_refused`

### tddy-code-restructuring — `tests/anchors_command_acceptance.rs`

- `anchors_items_emits_an_items_anchor_for_adjacent_module_items`
- `anchors_at_emits_an_item_anchor_relative_to_the_innermost_enclosing_item`

### tddy-index-daemon — `src/cli.rs` (unit)

- `anchors_at_carries_the_position_rather_than_items` — the daemon's command line carries `--at`
  into `AnchorsRequest.at`. The RPC's own behaviour is the library's `item_anchors`, which the two
  acceptance suites above drive against a live server; the daemon's fake-server anchors tests assert
  the `range` and the `anchor_json` a plan would carry, for `--items` and for `--at`.

### Unit tests

- `plan.rs`: `an_item_path_names_its_crate_and_its_segments`,
  `a_trait_qualified_segment_names_its_type_and_its_trait`, `an_item_path_of_one_segment_is_refused`,
  `an_item_path_displays_as_it_was_written`, `a_fingerprint_is_the_sha256_of_the_items_text`,
  `a_v2_header_carries_its_file_hints`, `an_item_anchor_round_trips_through_its_json`;
  `rejects_a_plan_written_for_a_different_schema_version` now uses `v:3` (v2 is a real schema).
- `item_anchor.rs`: relative → absolute ranges (3), module paths (4).
- `backends/rust/item_path.rs`: outline walk (4).
- `restructure_args.rs`: `--at` parsing (3).

`a_v1_plan_with_the_same_drift_is_still_refused` is the v1 regression guard.

## Technical Debt & Production Readiness

Green-phase notes (what the implementation settled, and what it left):

- **Continued runs refuse item anchors** — item anchors resolve against the tree the run starts on,
  and a run whose journal already completed operations no longer has it (the ledger would translate
  coordinates read from the edited tree a second time). Refused as
  `RestructureError::ItemAnchorsOnContinuedRun` (not a malformed plan: nothing is wrong with the
  plan). `TODO(plan-store)` in `runner/entry_points.rs`; the plan store's per-op refresh is what
  lifts it.
- **Resolution precedes the baseline compile gate** — `open_run_resolving_anchors` is the one order
  both apply loops use, so an item changed/absent/prefix refusal surfaces before minutes of
  `cargo check` and leaves no `.restructure/` behind.
- **`tddy-lsp`: `LspClient::root_uri()`** — `ItemResolver::resolve_item(file, item)` is given only a
  workspace-relative file, and a bridged backend holds only the client, so the client now reports the
  root it was initialized against. One accessor, no behaviour change.
- **Static `check` cannot examine item anchors**, so it reports each item-anchored operation as a
  finding (a plan of item anchors never passes a static check green); `check --deep` resolves them
  first. A static check has no server to resolve with.
- **No-resolver refusal** — this crate has no TypeScript backend (`Language` has only Rust), so
  "refuses by name" is the registry's: `LanguageBackend::item_resolver` defaults to `None` and the registry answers
  `UnsupportedOp { op: "item anchors" }`; a file with no backend at all is `NoBackend`.
- **The empty outline** — two parts. *Reproduced:* the first `settled_outline` looped while the
  outline was empty and `indexed` unset, and nothing on the anchors path sets `indexed`, so a file
  that genuinely defines nothing never returned (a live rust-analyzer: the acceptance test ran to
  its 180s token and failed `IndexingIncomplete`). *Still a hypothesis, not reproduced:* rust-analyzer is believed to answer
  `documentSymbol` for an open document with no symbols until its VFS has loaded (`readiness.rs`
  notes the server otherwise answers it from the syntax tree), and `ensure_indexed` treats an empty
  outline as "nothing to warm up" and leaves `indexed` false, so the first answer was taken as the
  file's. `settled_outline` now believes an empty answer only once the server has been *observed*
  quiescent (or `indexed`), refuses a degraded index, and is otherwise ended only by the caller's
  cancellation token — the daemon's anchors handler now carries one that fires when the request is
  dropped. A genuinely empty file (comments only) is pinned by an acceptance test and by unit tests
  of the decision. A server that never sends `experimental/serverStatus` still waits on an empty
  outline until its caller stops waiting. The cold path's `lsp server exited` is not addressed — it is a server exit, not
  an outline.
- **`walk_outline`** is the unit-test spelling of the walk; production calls the private `walk`
  and words its refusals with the file, which the signature does not carry.
- **`tddy-index-daemon` `code_index_service_acceptance`**: the anchors test's workspace now writes a
  `Cargo.toml` — an item path is rooted in a package, and the fixture had none. Assertion untouched.

- Draft-PR-contract stubs (all implemented; none remain): every `TODO(item-anchors)` in `plan.rs`, `item_anchor.rs`,
  `backends/rust/item_path.rs`, `ledger.rs`, `backends/rust.rs` (`unlowered_item_anchor`),
  `runner/entry_points.rs` (`item_anchors`), `restructure_args.rs` (`parse_position_range`),
  `tddy-index-daemon/src/queries.rs` (`anchor_json`), `tddy-tools/src/index_client.rs` (`--at`).
- Fingerprint text is defined as the item's whole lines (indentation included), not the exact
  server range — recorded on `Fingerprint`.

## Decisions & Trade-offs

- **Relative line counts from the item's first line, attributes and doc comments included** — the
  fingerprint covers the same text, so any edit that could shift a relative line also changes the
  fingerprint and is refused rather than mis-resolved.
- **Resolution at run open, ledger within the run** — reuses the proven in-run translation instead of
  re-resolving through LSP between every op; keeping later ops current *across* runs is the plan
  store's job.
- **No fallback search** when an item left its file — refused, per the repo's no-fallback rule; the
  plan store rewrites `file` when a move relocates it.
- **Trait-member syntax `<T as Trait>::m`** mirrors Rust's own qualified path.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-item-anchors-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run scoped tests (`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools -p tddy-lsp`) — 1120 passed, 0 failed, 9 ignored; CI for the rest
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
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-restructure-item-anchors-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps

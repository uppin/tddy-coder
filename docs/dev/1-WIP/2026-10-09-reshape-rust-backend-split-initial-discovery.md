# Initial discovery — #reshape 17/19 `rust-backend-split`

Companion to [the changeset](2026-10-09-reshape-rust-backend-split.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

## Exploration 1 — whole-work discovery (restructure backlog, 2026-10-09)


**Date:** 2026-10-09
**Scope:** `tddy-code-restructuring` backlog — missing engine features (priority), engine bugs,
oversized files, and the crate split into engine crates + a wiring package.

### Combined conclusions

- **Missing engine features dominate the cost of restructuring this crate with its own engine.**
  Cross-crate moves carry neither a module's directory children nor its sibling test modules, never
  widen what the origin still reaches, refuse grouped `use` lines and restricted `mod` declarations,
  and cannot target a crate that does not exist yet. Same-crate moves do not widen fields, impl
  members or what a reparented tree reaches. No operation moves `impl T` members to another module.
  `extract_method` drops comments and writes clippy-failing signatures — and it is the tool the
  function-size work would use.
- **Bugs** cluster in the tidy/facade layer (glob re-exports, orphan docs, rustfmt noise, facade
  visibility), apply robustness (`git mv` preflight, destination compile gate, no wait deadline),
  and `check`/`apply` parity (stranded siblings, multi-seam `extract_module`).
- **Sizes:** 3 files over 500 production lines; 25 functions over 60 lines, 71 at 41–60.
- **Crate split:** only `crate_move ↔ registry`/`item_anchor` and `runner ↔ console` are real cycles.
  `backends/rust` cannot be split across crates while operations are inherent `impl RustBackend`
  blocks — the backend needs a seam (trait / session handle) or stays one crate.
- **Not restructure:** read-lints via warm index, daemon anchors-twice (index-daemon), daemon
  `runtime.rs` / tool-engine sizes, jail e2e test, `reexport: none` recipe note.
- **Closable as already done:** macro-expansion research note, connection-service-split D6–D10.

### Exploration 1 — backlog batch B (2026-10-04 … 2026-10-08 hand-workaround records)

Paths relative to `packages/tddy-code-restructuring/src/` unless noted.

1. **move-cluster-to-crate-leaves-a-modules-directory-children-behind** — BUG, OPEN. `resolve_cluster` adds one `FileEdit::Rename` per member (`crate_move/cluster.rs:133`); `crate_move/module_files.rs` (`children_directory`, `files_of`) exists but only `module_reparent`, `item_move`, `repoint_facade` use it. Sub-items: dry-run count ≠ apply renames; destination manifest misses crates moved code names (`libc`); `check --deep` doesn't report stranded children; `also` children land flat; stranded doc comment. Size M–L. **Blocks cross-crate moves: YES** — this crate is full of `foo.rs` + `foo/`.
2. **move-item-copies-the-whole-use-header** — BUG, PARTLY FIXED (reachability filter landed; still relies on `runner/tidy.rs`). M. Does not block.
3. **move-item-does-not-widen-fields-or-impl-members** — MISSING-FEATURE, OPEN (no E0616/E0624 handling in `item_move`). Also: nested `use` group refused (`item_move/sites.rs:335`), keyword-above-name, one-line inline destination, relative range anchor, `#[path]` lib+main, fn-body `use` counted as module scope. M. Same-crate.
4. **reexport-outside-limits** — MISSING-FEATURE, OPEN: custom `[lib]`/`[[bin]] path` unread (`item_move/outside.rs:9`); no per-item module facade; one manifest walk per referring file. S each. Does not block.
5. **reparent-module-does-not-widen-what-the-moved-tree-reaches** — MISSING-FEATURE, OPEN (`module_reparent/visibility.rs` only computes the declaration `Landing`; `module_reparent/assemble.rs:203` keeps keywords). M. Indirect blocker (regrouping before carving; crate uses `pub(in crate::backends::rust)` heavily).
6. **reparent-module-first-cut-limits** — MISSING-FEATURE, mostly OPEN: inline new parent, inline `mod x {}`/`#[path]`, declaration on own lines, undeclared files stay, emptied dirs stay (no `remove_dir` anywhere in `src`), grouped/unqualified `use` refused, cfg-gated refs, `a.rs`+`a/mod.rs`; docs wording on `glob`. S each, M overall. Partly blocks.
7. **session-restructure-tools-grew-two-oversized-files** — FILE-SIZE, NOT-RESTRUCTURE (`tddy-daemon/src/runtime.rs` 1,677, `tddy-tool-engine/src/lib.rs` 873).
8. **session-restructure-tools-no-jail-end-to-end-test** — NOT-RESTRUCTURE (daemon/lsp-executor test gap).
9. **apply-did-not-return-after-a-clean-deep-check** — BUG, PARTLY FIXED: heartbeat in `backends/rust/wait.rs:1-7`, "adds no deadline" (wait.rs:9); root cause never investigated. M. Intermittent blocker.
10. **no-operation-re-points-a-calls-receiver-or-writes-a-delegator** — `repoint_call` FIXED (`backends/rust/repoint_call*`); `leave_delegator` OPEN — parses (`plan/codec/retarget_fields.rs:106`) but refused `UnsupportedOp` (`backends/rust/retarget_impl.rs:38-43,62`); dead-wrapper report open. M.
11. **no-record-of-what-an-apply-executes** — spawn log + daemon exit line FIXED (PR #590); op id on spawn records OPEN. S.
12. **same-crate-moves-limits-found-moving-lifecycle** — emptied dirs; widening through `pub use` chain; aliased `use` in clash check (`item_move/bindings.rs:35`); `super::Name` through glob; import placement; one module per `move_item`. S each. Same-crate.
13. **item-move-assemble-past-500** — FILE-SIZE, OPEN (507 prod lines). S.
14. **retarget-impl-refuses-a-relative-import-of-its-target-type** — BUG, OPEN (`retarget_impl/imports.rs:39-55` text compare; reuse `import_text.rs`). S.
15. **hand-split-grouped-use-lines-before-the-agents-cluster-move** — BUG record: grouped `use` with different qualifiers refused (`crate_move/header.rs:241`, `crate_move/test_binary.rs:816`; `repoint_facade` Rule S splits groups but `crate_move` doesn't); glob-facade path to co-mover reads as "stays behind" (likely open); `pub(in crate::origin)` counted as naming origin crate; re-exporting crate preferred over defining crate for manifest edges. M. **Blocks: YES.**
16. **hand-widened-mod-declarations-before-engine-moves** — BUG, OPEN: `crate_move/manifest_edits.rs:6-19` `module_declaration` strips only `"pub "`; facade writer keeps no narrower visibility. S. **Blocks: YES** (this crate uses `pub(crate) mod`). Sibling of `move-to-crate-does-not-read-a-restricted-mod-declaration`.
17. **narrow-the-items-the-carve-moves-widened-to-pub** — HYGIENE + missing "narrow visibility" op; items outside this package. M. Every split recreates this debt.

### Exploration 2 — backlog batch C (2026-10-08 engine defects), code issues, dependency picture

No engine commit has touched `packages/tddy-code-restructuring/src` since 2026-10-08 (last engine commit `480c84481`, #595 `#sharpen 8/8`).

#### docs/dev/todo
1. **apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root** — BUG (diff noise), OPEN: `runner/tidy/format.rs:49-68` `format_touched` formats whole files. S–M. No block.
2. **cluster-move-leaves-unused-reexports-and-an-orphan-doc** — BUG, OPEN: `runner/tidy/gating.rs:56` returns `None` for a glob, so glob re-exports never tidied; doc comments not carried. M. Lint gate red after splits.
3. **cluster-move-misses-body-paths-and-writes-self-referencing-test-reexports** — BUG, OPEN: `crate_move/moving/facade_writer.rs:152-166` `parent_reexport_edits` always writes `{destination.extern_name}::{module}` (self-facade, E0432); struct-update `..crate::old::f()` not re-pointed (general body paths fixed by #540). M. **Blocks.**
4. **cluster-move-strands-test-modules-of-the-moved-code** — MISSING-FEATURE, OPEN: sibling `#[cfg(test)] mod x_tests;` not carried. M. **Blocks** (this crate has many `*_tests.rs`).
5. **module-move-strands-its-directory-child-and-the-cluster-leaves-a-dangling-self-reexport** — BUG, OPEN: `crate_move/module_files.rs:19` `children_directory` unused by `cluster.rs`/`moving.rs`. M–L. **Blocks critically** — `crate_move/`, `runner/`, `plan/`, `plan_store/`, `backends/rust/`, `spawn_record/`, `verify/` all have directories.
6. **move-cluster-ignores-also-members-that-are-directory-children-and-their-crates** — BUG + MISSING-FEATURE (create-crate): `crate_move/destination.rs:32` refuses a `to` that is not an existing crate. L. **Blocks** — every new engine crate needs a hand skeleton.
7. **move-cluster-refuses-a-grouped-use** — MISSING-FEATURE, OPEN: refusal in `crate_move/header.rs:65,200,206,238` `one_use_per_path`; Rule S logic in `backends/rust/repoint_facade/group.rs` not reused. M. **Blocks.**
8. **move-item-miswrites-a-facade-path-in-a-moved-impl-block** — BUG, very likely OPEN (`super::<crate>::…`, E0433); tidy silently skipped when compile gate fails. M. Partly blocks.
9. **move-item-reexport-none-edits-other-packages-callers** — NOT-RESTRUCTURE (works as documented; recipe/docs). S.
10. **move-to-crate-does-not-read-a-restricted-mod-declaration** — BUG, CONFIRMED OPEN: `crate_move/manifest_edits.rs:12-14` strips only `"pub "`. S. **Blocks.**
11. **move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs** — BUG/MISSING-FEATURE, OPEN: no widening in `crate_move/` (only a comment at `header.rs:34`); reached items listed by survey (`runner/rehearsal.rs:130-140`) but unused; reuse `backends/rust/visibility.rs`. M. **Blocks.**
12. **move-to-crate-leaves-pub-crate-items-the-origin-still-uses** — superset of #11 incl. fields/methods of reached types; hundreds of hand widenings per milestone. L. **Biggest cost.**
13. **move-to-crate-misses-a-crate-named-only-by-an-attribute-macro** — BUG, probably OPEN: `crate_move/source_scan/*`, `crate_move/manifest_edits.rs:141` `dependencies_of`. S–M.

#### packages/tddy-code-restructuring/docs/code-issues
- **broken-restructure-anchors-empty-outline** — BUG, unclaimed, partly fixed by #537 (`settled_outline` `backends/rust.rs:1698`, `places_of` `:2380`); warm refusal + cold `lsp server exited` not re-measured. M. Blocks item-anchored `extract_module` used to shrink oversized files.
- **complexity-rust-facade-lines** — HYGIENE, `backends/rust/facade.rs:151` depth 5. S.
- **dead-code-plan-filehint-modified** — HYGIENE, `plan.rs:133`, writer `plan/codec/file_hint.rs:8-13`. S.
- **oversized-file-backends-rust** — FILE-SIZE, ~2,900 production lines; remainder is `impl RustBackend` members + trait impls. L.
- **oversized-file-test-binary** — FILE-SIZE, `crate_move/test_binary.rs` 967 lines; lexical scanner (`Prose`, `readable_spans`) also used by `backends/rust/early_return.rs`. M.

#### Module dependency picture (`crate::<mod>` counts)
```
edit -> (none)       spawn_record -> (none; backends ref is a doc link)     verify -> (none)
apply -> edit, spawn_record(1)          overlay -> apply, edit
plan -> apply(7: hash_file), edit       item_anchor -> crate_move(2: declared_package_name), edit, plan
registry -> crate_move(1: ModuleReferences), item_anchor(3), edit, overlay, plan
crate_move -> apply, edit, overlay, plan, registry(11)
ledger -> edit, item_anchor, plan       journal -> apply, edit, ledger, plan
plan_store -> apply, edit, item_anchor(7), ledger, plan
backends -> apply(2), crate_move(30), edit(46), item_anchor(12), plan(25), registry(24), spawn_record(2)
console -> edit, plan, plan_store, runner(1), verify
runner -> apply, backends(11), console(5), crate_move, edit, item_anchor, journal, plan, plan_store, registry, spawn_record, verify
restructure_args -> edit, item_anchor, plan, runner
restructure_cli -> backends, console, item_anchor, restructure_args, runner, spawn_record
```
Cycles: **crate_move ↔ registry** (11 / 1, `registry.rs:6` `ModuleReferences`) and **item_anchor → crate_move → registry → item_anchor** — CONFIRMED. `apply→spawn_record→backends→apply` — REFUTED (doc link only). `plan↔plan_store` — REFUTED (doc comment only). Unlisted: **runner ↔ console** (`console.rs:26` uses `runner::{Finding, Outcome, …}`).

`runner` uses concrete `RustBackend` only in `runner/entry_points.rs:8,95,133`; elsewhere only `ProgressSink` (`backends/rust.rs:499`), `discard`, `WAIT_HEARTBEAT` (`backends/rust/wait.rs:21`), `human_delta` — movable down.

**`backends/rust/` operations are inherent `impl RustBackend` blocks** in child modules (`item_move.rs:53`, `retarget_impl.rs:77`, `repoint_call.rs:48`, `repoint_facade.rs:45`, `module_reparent.rs:43`, `signature.rs:81`, `imports.rs:18`, `documents.rs:23`, `readiness.rs:42`, `item_path.rs:257`) calling `self.start`, `self.references_at`, `self.settled_outline` and sibling-impl methods. **Splitting `backends/rust` across crates 3/4/5 is impossible without first turning `RustBackend` methods into a trait or free functions over a session handle** (no inherent impl in another crate).

Dependents: `tddy-tools`, `tddy-index-daemon` (deps + dev-deps), `tddy-daemon-rpc`.

### Exploration 3 — function sizes (added to scope by the developer, 2026-10-09)

Thresholds from `.agents/commands/analyze-clean-code.md`: ≤20 excellent, 21–40 acceptable, 41–60
needs attention, **>60 must refactor**. Measured with a brace-matching scan of production `.rs`
under `packages/tddy-code-restructuring/src` (skips `*_tests.rs`, `tests.rs`, and everything after
an inline `#[cfg(test)] mod`; strips string/char literals and line comments; ±a few lines).

**1,409 functions; 25 over 60 lines; 71 at 41–60.** Top of the list (lines, file:line, fn):

```
206 plan/codec.rs 246 parse_op
163 backends/rust.rs 1245 resolve_opening
159 runner/entry_points/store_run.rs 237 apply_held_plan
124 backends/rust.rs 1520 assisted_edit
115 runner/entry_points/check_entry_points.rs 207 check_plan
95 crate_move/source_scan/sighting_walk.rs 33 sightings
93 backends/rust.rs 289 assist_for
91 backends/rust/item_move/sites.rs 94 edits_for_file
87 backends/rust.rs 735 start
82 backends/rust/item_move/assemble.rs 71 assemble
76 plan_store/refresh.rs 61 refreshed
73 restructure_args.rs 206 options_for
73 backends/rust/item_move.rs 55 move_items
71 backends/rust/item_move/assemble.rs 206 visibilities
71 backends/rust.rs 1022 offered_assist
70 crate_move/cluster/stranded.rs 54 stranded_siblings
70 backends/rust/retarget_impl.rs 79 retarget_impl
66 crate_move/cluster.rs 111 resolve_cluster
66 backends/rust/visibility.rs 37 restore_visibility
66 backends/rust/item_move/sites.rs 324 rewrite_statement
66 backends/rust.rs 1124 check
65 backends/rust/import_text.rs 18 choose_import
64 crate_move/source_scan/module_items.rs 32 items_of_module
64 backends/rust.rs 823 request
62 backends/rust/imports.rs 103 next_import
60 backends/rust/readiness.rs 147 await_answer
60 backends/rust/item_move/rebase.rs 99 path_edit
60 backends/rust/item_move/assemble.rs 448 into_destination
60 backends/rust/item_move/assemble.rs 348 moved_text
60 backends/rust.rs 1933 chain_module_to_file
59 backends/rust/repoint_facade/group.rs 20 split_or_reprefix
```

Concentration: `backends/rust.rs` holds 7 of the 25 (`resolve_opening` 163, `assisted_edit` 124,
`assist_for` 93, `start` 87, `offered_assist` 71, `check` 66, `request` 64);
`backends/rust/item_move/` holds 8; `runner/entry_points/` 2 (`apply_held_plan` 159,
`check_plan` 115); `plan/codec.rs::parse_op` 206 is the largest (an op-name match — a table
dispatch candidate).

### Exploration 4 — backlog batch A (2026-09-09 … 2026-10-04)

1. **macro-expansion-as-a-restructure-operation** — research note recommending NOT building it; prerequisites (`placeholder_checks.rs:20`, `readiness.rs:59`) present. Closable record.
2. **restructure-defects-from-the-connection-service-split** — D6–D10 all FIXED (`facade.rs:76,124,135`, `module_text.rs:256`, `import_text.rs:18`); caveat tracked as 09-24 M. Closable.
3. **restructure-defects-from-the-first-cross-crate-move** — 4 open: (1) `move_items_to_crate` MISSING (L); (2) `git mv` of an uncommitted file fails half-edited (`apply.rs:162`, no `ls-files` preflight) S; (3) cross-crate widening not reported S; (4) caller re-point spliced inside grouped `crate::{…}` M (unverified). Self-dependency manifest defect unconfirmed. **Blocks.**
4. **extract-module-cannot-see-sibling-seams-in-one-plan** — PARTLY FIXED (`inline_paths.rs` header 9–12; `reach_of` `rust.rs:1833`); no projected-module-tree survey; `check --deep` doesn't type-check the final tree. M–L. Blocks multi-seam file splits.
5. **restructure-apply-gaps-from-the-lifecycle-destructure-run** — H, X(1st half), G(likely) FIXED; OPEN: I (`use Trait as _` not carried, `rust.rs:4038,4786`), J (=4), K (`extract_method` signature names an unimported type; `restore_imports` only on `extract_module` path `rust.rs:1615`), M (test children lose `use super::*` names; `pub use` narrowed), W (`readiness.rs:69` warm-up skip). Compiler-guided import repair is one mechanism for G/I/K/M. L.
6. **restructure-apply-leaves-the-lint-gate-red** — N1, N2, N4, N3-unused FIXED (`runner/tidy.rs`); N3 missing-import half OPEN (same oracle as 5 I/K).
7. **restructure-extract-drops-comments-and-writes-clippy-failing-signatures** — P (comments dropped; `verify.rs:558` only reports) OPEN; Q (`ptr_arg`, leftover `mut`, `Ok(())` tail, `x: x`) OPEN; R (hang on range starting at `{`/comment; `wait.rs:9` no deadline) OPEN; S (`let … else`) open; T FIXED; U unverified; V index-daemon. M–L. **Prerequisite for function-size work** (extract_method is the tool).
8. **restructure-snapshot-cannot-rebase-a-stale-plan** — item anchors FIXED (`plan_store/refresh.rs:153`); v1 range anchors OPEN (`refresh.rs:69`). S–M or retire v1.
9. **restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter** — `read_fields_through` MISSING. M. Needed to move method bodies off a type that stays.
10. **move-to-crate-leaves-a-nested-modules-parent-glob-dangling** — glob-visible items not counted as reached (`crate_move/survey.rs`, `runner/rehearsal.rs:130`). S–M. Mild blocker.
11. **test-binary-move-cannot-see-through-a-glob-facade** — compile gate omits destination package (`runner/compile_gate.rs:52-65` `owning_packages`). S.
12. **static-check-cannot-verify-item-anchors** — `check_entry_points.rs:364` `unresolvable_without_a_server`. S.
13. **the-daemon-anchors-path-resolves-an-item-twice** — `tddy-index-daemon/src/queries.rs:89,97`. S. Touches `runner::item_anchors`.
14. **glob-reexport-is-narrower-than-the-moved-items-need** — `facade.rs:124-130` `widest_visibility` ignores consumers; tidy skipped when compile gate fails. S–M. **Blocks** (public surface across seams).
15. **leftovers-of-the-live-plan-carve-and-tooling-pass** — (1) `impl RustBackend` member groups out of `rust.rs` L OPEN; (2) named facade lists test-only names M; (3) `test_binary.rs` size M; (4) stale-plan "item changed" message `lib.rs:106` S; (5) untested shapes; (6) verify `)` reflow S; (7) split drops unreferenced `pub` item (`facade.rs:170-173`) S–M; (8) no replay corpus M.
16. **restructure-rust-backend-grows-with-every-live-plan-node** — FILE-SIZE, same as 15(1). 2,705 → ~2,983.
17. **stranded-sibling-finding-reads-only-the-use-header** — `crate_move/header.rs:29,37` `TODO(check-parity-header)`, `cluster/stranded.rs:142`. S. **Blocks** cluster moves.
18. **read-lints-is-refused-through-the-warm-index** — NOT-RESTRUCTURE (`tddy-lsp-executor/src/index_backed.rs:179`).

**Impl-member seam does not exist.** `oversized-file-backends-rust.md:24`: `check --deep` refused moving
`impl RustBackend` members because rust-analyzer cannot put a `mod` inside an `impl` body. No operation
moves methods of one `impl T` block into an `impl T` block in another module. This is the missing
feature behind `rust.rs`'s remaining ~2,983 production lines.

**Files over 500 production lines** (counted to the first inline `#[cfg(test)] mod`): only three —
`backends/rust.rs` 2,983, `crate_move/test_binary.rs` 967, `backends/rust/item_move/assemble.rs` 507.

## Exploration 2 — rust-backend-split (what `backends/rust.rs` holds, where each part goes, and which operation moves it)

Paths are relative to `packages/tddy-code-restructuring/src/` unless noted. Line numbers are master at `4a5c42b1b`.
Production lines are counted by `#reshape` 15's rule: every line except the lines of a `#[cfg(test)]` item.

### E2.1 — The file and its count

- `backends/rust.rs` has 5,566 lines. The inline `#[cfg(test)] mod tests` starts at `:3009` and runs to the end (2,557
  lines, with a nested `use super::*` module at `:5476`).
- Above it, 50 lines are `#[cfg(test)]` items: two `#[cfg(test)] use server_process::…` pairs at `:511-514`, and 23 pairs
  in the `mod`/`use` hub at `:2900-2968` (`#[cfg(test)] use import_text::choose_import;` and the like). They exist only so
  that the inline tests' `use super::*` sees names defined in the children.
- **Count: 2,958 production lines** (3,008 to the first inline test module, minus those 50). Node 15's discovery says
  2,959. Neither the brief's 2,983 nor the code-issue record's 3,008 is the items-rule number.
- What lands below this node before it starts (from the wave-1 changesets): node 12 deletes `anchor_for` (`:1191-1205`),
  `anchor_opening` (`:1409-1445`), `module_outline` (`:1726-1739`), `OutlineItem` with its impl, `places_of` and
  `refuse_non_adjacent` (`:2354-2415`), about 130 lines. Node 3 moves `attached_trivia_starts_at` (`:2416-2443`, 28
  lines) to `crate_move/source_scan.rs` by engine. Nodes 2, 4, 6, 10 and 13 add wiring: fields (node 2 `projection`,
  `staged_projection`; node 10 `bounds`), a builder (node 10 `with_silence_bounds`), `mod` lines (`extracted_fn`,
  `read_fields_through`, `impl_move`, `same_crate_dispatch`) and dispatch arms that node 13 folds. **Expected at the base:
  about 2,850.** It is re-measured there with `restructure lines`.

### E2.2 — Inventory by responsibility

The file has four inherent blocks and three trait impls:
- `impl RustBackend` #1 at `:516-1093`, 24 members;
- `impl Drop` at `:1095`, `impl LanguageBackend` at `:1104-1226`, `impl ModuleReferences` at `:1233-1241`;
- `impl RustBackend` #2 at `:1243-1483`: `resolve_opening`, `anchor_opening` and `outside_references_opening`;
- `impl RustBackend` #3 at `:1485-2310`, 23 members.

Everything else is free items. Every production line is assigned below. Counts include doc comments and the blank line
before each item. Total: 2,958.

| Group (responsibility) | Items (file:line) | Prod lines | Target module | Operation |
|---|---|---:|---|---|
| **The type and its construction** | module doc `:1-9`, `use` header `:11-28`, `mod` list and hub `:30-63`, `:510-515`, `:2899-2971`; `SYMBOL_KIND_*` `:64-73`; `INDEXING_POLL`, `UNRESOLVED_TOKEN`, `IMPORT_TITLE` `:385-404`; `struct RustBackend` `:425-490`; `ProgressSink`, `discard`, `discard_line` `:492-508`; block #1 constructors `new` … `from_lsp_client` `:517-635` | 326 | stays in `rust.rs` | — |
| **Shared vocabulary** | `visibility_in`, `is_identifier_char`, `is_identifier`, `covers` `:2804-2846`; `failure`, `seam_refusal`, `server_defect` `:2976-3007` | 79 | stays in `rust.rs` (39 child files name one of these; F5) | — |
| **Routing** (`RustBackend` as a `LanguageBackend` and `ModuleReferences`) | `SUPPORTED` `:75-101`; `impl Drop`, `impl LanguageBackend`, `impl ModuleReferences` `:1095-1241`; block #2 `resolve_opening` `:1244-1407`, `outside_references_opening` `:1447-1482` | 381 | new `language_backend.rs` | `move_item` (the constant and the three trait impls, as one run); `move_impl_members` (block #2, all of it) |
| **The assist catalogue and its offer** | `Assist` `:103-132`; `titled` `:166-182`; `Placeholder` and its impl, `assist_for` `:271-381`; `offered_titles` `:2463-2475`; `absent_assist`, `incomplete_assist_index` `:2486-2538`; `context_for` `:2848-2855`; members `inference_ready_at`, `assist`, `offered_assist` `:989-1092` | 347 | new `assists.rs` | `move_item` ×6, `move_impl_members` ×1 |
| **Transport** (JSON-RPC and the session's start) | members `workspace_root`, `take_id` `:637-654`; `start` … `did_change` `:734-987`; `SETTLE_POLL` `:388`; `CONTENT_MODIFIED`, `CONTENT_MODIFIED_RETRIES` `:420-423`; `unsettled` `:2540-2552` | 293 | new `transport.rs` | `move_impl_members` ×2, `move_item` ×3 |
| **Assist results into workspace edits** | members `chain_module_to_file`, `edit_for`, `multi_file_assist` `:1920-2062`; `convert_change` `:134-164`; `Produced`, `edits_the_parent` `:2331-2353`; `MOD_KEYWORD`, `caret_at_module`, `whole_word` `:2554-2603`; `document_changes`, `relative_path` `:2857-2872`; `edits_in` `:2889-2898` | 280 | new `assist_edits.rs` | `move_impl_members` ×1, `move_item` ×5 |
| **The extraction pipeline** | members `assisted_edit` `:1519-1643`; `prune_assist_imports`, `unresolved_names` `:1873-1918`; `extract`, `rename_placeholder` `:2253-2309` | 231 | new `extraction.rs` | `move_impl_members` ×3 |
| **Seam survey queries** | members `survey_moved_items` `:1645-1677`, `survey_impl_members` `:1741-1803`, `reach_of` `:1832-1871`; `seam_trace` `:2312-2329`; `visibility_at` `:2786-2802` | 176 | new `seam_reach.rs` | `move_impl_members` ×3, `move_item` ×2 |
| **Reference queries and symbol walks** | members `references_outside` `:1486-1517`, `references_at` `:1805-1830`; `path_reached_within`, `PathReached`, `collect_path_reached`, `whole_of`, `character_column` `:2680-2784` | 166 | new `references.rs` | `move_impl_members` ×2, `move_item` ×1 |
| **Symbols** (locate and rename) | members `anchor_range`, `rename_symbol`, `locate_symbol` `:2138-2251`; `find_symbol` `:2638-2655` | 133 | new `symbols.rs` | `move_impl_members` ×1, `move_item` ×1 |
| **Handshake** | `client_capabilities`, `BYTE_ENCODING`, `negotiated_encoding`, `server_settings` `:184-269`; `refuse_foreign_encoding` `:2445-2461` | 103 | new `handshake.rs` | `move_item` ×2 (the `pub` run with `reexport: outside`, F2) |
| **Waiting** | members `keep_waiting`, `beat`, `waited_on`, `incomplete_index` `:656-732`; `CANCEL_CHECK` `:390-395`; `first_symbol_position` `:2657-2671` (only `readiness.rs` calls it) | 100 | existing `readiness.rs` | `move_impl_members` ×1, `move_item` ×2 |
| **The outline** | members `settled_outline`, `outline_is_the_servers_answer` `:1679-1725`; `outline_is_empty` `:2477-2484` | 56 | existing `documents.rs` | `move_impl_members` ×1, `move_item` ×1 |
| **LSP geometry** | `LspEdit`, `LspPoint` and its impl `:2605-2636`; `path_of` `:2673-2678`; `relative_to` `:2874-2887`; `uri_of` `:2972-2974` | 56 | existing `lsp_edits.rs` | `move_item` ×4 |
| **Signature dispatch** | member `rewrite_signature` `:2064-2088` | 26 | existing `signature_rewrites.rs` | `move_impl_members` |
| **Return-type assist** | member `wrap_or_unwrap_return_type` `:2090-2136` | 48 | existing `return_type.rs` | `move_impl_members` |
| **Import pass bound** | `IMPORT_PASSES` `:406-418` (only `imports.rs` reads it) | 14 | existing `imports.rs` | `move_item` |
| *Deleted by node 12* | see E2.1 | 115 (+15 in `impl LanguageBackend`) | — | — |
| *Moved by node 3* | `attached_trivia_starts_at` | 28 | `crate_move/source_scan.rs` | — |

**By operation:** `move_impl_members` moves about 1,470 lines (17 runs). `move_item` moves 148 lines of trait impls (one
run) and about 790 lines of free items (30 runs). That is 48 plan lines in 11 plans (changeset § Plans).

**Node 13's E2.2 "this op alone cannot bring `rust.rs` under 500" holds.** The free items are about 790 movable lines, not
"~1,200". The rest of the ~1,200 it named is the type, its constructors and the hub, and those stay.

### E2.3 — Contiguity decides the number of plan lines

An `items` anchor must be contiguous (plan-schema § Item anchors: "only blank lines may separate them"). `move_impl_members`
takes one run of one block (node 13, R1). Responsibilities interleave:
- Block #1, in order, is constructors, then `workspace_root` and `take_id` (transport), then four waiting members, then
  `start` … `did_change` (transport), then three assist members.
- Block #3 alternates between six destinations: references, extraction, seam survey, outline, references, seam survey,
  extraction, assist edits, signature, return type, symbols, extraction.
- The free items of one group sit in up to six places, in two regions (`:60-423` and `:2311-3007`).

The leftovers todo § 1's five runs (`session`, `transport`, `extraction`, `assist_ops`, `backend_impl`) are spans of
lines, not responsibilities. Its `assist_ops` (`prune_assist_imports` … `rename_placeholder`) mixes the import repair of
extraction, multi-file edit conversion, signature dispatch, the return-type assist, symbol renaming and the placeholder
rename. Cutting by span saves about 30 plan lines and produces modules that node 18 would have to split again (F3).

### E2.4 — Visibility the moves create (what node 1 and node 13 must widen)

- **Moved members.** Each private member called from outside its new module becomes `pub(super)` (node 13 R6 a). Every
  destination is a child of `backends::rust`, so stayed members, fields of `RustBackend` (`:425-490`, all private) and
  private items of `rust.rs` need no widening: a child sees its ancestors' private items (node 13 E2.3, bound R6 b–d).
- **Moved structs with private fields read elsewhere** (E0616 unless widened). This is node 1's field widening, consumed
  directly:
  - `Assist`: fields read in `resolve_opening` `:1381` (`multi_file`), `assisted_edit` `:1538`, `:1545`, `:1579`, and
    `return_type.rs:57`. `return_type.rs` also builds `Assist { … }` literals, which need every field visible.
  - `Placeholder`: read by `introduced.rs:14`.
  - `Produced<'a>` (`:2336`): built in `resolve_opening` `:1396`, read in `assist_edits`.
  - `LspPoint` (`line`, `character`) and `LspEdit`: read in `item_path.rs`, `readiness.rs`, `item_move.rs`,
    `item_move/outline.rs`, `repoint_call/sites.rs` and `lsp_edits.rs`.
  - `LspPoint::read`, a private associated fn: node 1's member widening.
  - `PathReached` (`:2699`): read by `seam_survey.rs:7`.
- **This is a direct consumption of node 1**, not only one through node 13. The brief lists real parents 13, 2 and 3.
  Node 15 added `1→15` for the same reason. The line order already puts node 1 below this node, so nothing reorders.

### E2.5 — The public surface that must not change

- From outside the module, `backends::rust` is reached for `RustBackend` (and `::from_lsp_client`), `ProgressSink`,
  `discard`, `WAIT_HEARTBEAT`, `human_delta` and `ServerChatter`:
  - inside the crate: `runner.rs:198`, `runner/*.rs`, `restructure_cli.rs:28`, `spawn_record.rs:6`;
  - other crates: `tddy-index-daemon/src/{index,graph,apply,service,main}.rs` and its tests.
  All of these stay in `rust.rs`.
- `client_capabilities` and `server_settings` are `pub`. `lib.rs:26` re-exports them, and five `tddy-index-daemon` tests
  call them as `tddy_code_restructuring::client_capabilities()` (`wait_heartbeat_acceptance.rs:42`,
  `live_plans_acceptance.rs:28`, `warm_index_production.rs:53`, `spawn_record_acceptance.rs:24`,
  `activity_log_acceptance.rs:78`).
- With `reexport: none`, `move_item` re-points **every** caller the server finds, including those in other packages. That
  would spell `…::backends::rust::handshake::client_capabilities` in `tddy-index-daemon` and force `pub mod handshake`.
  With `reexport: outside` the callers in the library are re-pointed (`lib.rs:26`), the outside ones are never edited,
  and one named `pub use handshake::{client_capabilities, server_settings};` line stays in `rust.rs`. That line keeps
  today's second public path `backends::rust::client_capabilities` (plan-schema § Same-crate moves, `reexport: outside`).
  This is F2.

### E2.6 — How children reach `rust.rs` today (blast radius of `reexport: none`)

- 30 child files carry a `use super::…` of items in `rust.rs`. Several are single-level groups:
  - `readiness.rs:9-12`: `first_symbol_position, path_of, relative_to, seam_refusal, server_defect, LspPoint, RustBackend, WaitStage, Waiting, INDEXING_POLL`;
  - `signature.rs:12-15`;
  - `imports.rs:9-14`: names `titled`, `IMPORT_PASSES`, `IMPORT_TITLE`;
  - `item_move.rs:37`: `lsp_edits, relative_to, seam_survey, server_defect, uri_of, LspPoint, RustBackend`;
  - `item_path.rs:10`, `module_reparent.rs:26`, `return_type.rs:1` (`Assist`), `introduced.rs:14` (`Placeholder`),
    `lsp_edits.rs:5-9` (`LspPoint`, `LspEdit`), `seam_survey.rs:1-7` (`SYMBOL_KIND_IMPL`, `covers`, `PathReached`).
- `move_item` takes a moved name out of a single-level group into a `use` of its own (`item_move/sites.rs:320-336`). It
  refuses only a **nested** group (`:374`). No child nests a group of `rust.rs` names, so no refusal is expected from this
  rule. If one appears, the run stops and the developer is asked.
- 39 child files name `failure`, `seam_refusal` or `server_defect`. That is why the vocabulary stays (F5): moving 79 lines
  would re-point 39 files to gain budget that is not needed.
- The inline tests reach everything through `use super::*` (`:3011`). `move_item` re-points a bare name "through an
  import" by inserting a `use` in the site's scope (`item_move/sites.rs:1-6`). So the inline tests gain `use` lines inside
  `mod tests`, which are test lines and are not counted. Moved private methods called from the tests (for example
  `RustBackend::rewrite_signature`) become `pub(super)`, which covers `backends::rust::tests` (node 13 E2.6).

### E2.7 — Risks found by reading, not reproduced

- **A `move_item` run made only of trait impls is untested.** `item_move/outline.rs:63-89` accepts a run whose symbols are
  all `impl` blocks: `Run.items` is empty, because an impl has no identifier name. No suite moves a trait impl:
  `grep " as [A-Z]…>"` over `tests/` finds only anchor and plan-line suites. Node 13's E2.2 note ("`move_item` of the
  block lines already") is unverified. The first `check --deep` of plan K is the probe. A refusal stops the run.
- **Join landings.** `readiness.rs` and `documents.rs` each hold exactly one `impl RustBackend` (`grep -c '^impl
  RustBackend'` = 1), so node 13's join rule appends there. `signature_rewrites.rs` and `return_type.rs` hold none, so a
  new block is opened.
- **Cycles from free-item placement.** `first_symbol_position` is called only by `readiness.rs`. Placing it in
  `documents.rs` would create a `readiness → documents` `use` edge, opposite to the `documents → readiness` method calls
  of `settled_outline`. It goes to `readiness.rs`.
- **The inline tests stay** (2,557 lines plus the 46 `#[cfg(test)] use` hub lines). No operation splits an inline test
  module, and test lines are not counted, so the budget is met without them (F6, proposed todo).
- **No `macro_rules!`** in `rust.rs` (`grep macro_rules` finds none), so node 13's textual-macro limit does not apply.
- **`server_process.rs` (92) is not cohesive.** #539 left `without_hollow_imports` and `binds_nothing` (import-tidy
  helpers, `:21-45`) beside `Server` and the toolchain lookup. The handshake does not join it (F4). The misplacement is a
  proposed todo.

### E2.8 — Where each function on the >60-line list lands (none changes body)

| Function | Lines | From | To |
|---|---:|---|---|
| `resolve_opening` | 163 | `rust.rs:1245` | `language_backend.rs` |
| `assisted_edit` | 124 | `:1520` | `extraction.rs` |
| `assist_for` | 93 | `:289` | `assists.rs` |
| `start` | 87 | `:735` | `transport.rs` |
| `offered_assist` | 71 | `:1022` | `assists.rs` |
| `check` (`LanguageBackend`) | 66 | `:1124` | `language_backend.rs` |
| `request` | 64 | `:823` | `transport.rs` |
| `chain_module_to_file` | 60 | `:1933` | `assist_edits.rs` |

Node 19 (`fn-sizes-backend`) re-anchors them in their new files.

### E2.9 — The crate split this layout prepares (stack 2)

`docs/dev/todo/2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md:17-19` puts `readiness`, `wait`,
`server_process` and `documents` in crate 3 (Rust support) and "the `backends/rust.rs` dispatcher" in crate 5. Its open
question, "Is `backends/rust.rs` a dispatcher into every operation? Not verified" (`:39`), is answered here: today it is
not. Operations call back into its members (`self.start`, `self.request_settled`, `self.references_at`, …). After this node
the session plumbing sits in `transport`, `readiness`, `documents`, `references` and `symbols`, with `handshake` and
`lsp_edits` free of `RustBackend`. The dispatcher sits alone in `language_backend`. The edges that must not exist
(changeset § Final Checklist) keep the support layer from reaching up into the operations. That is the seam node 18 turns
into a session handle.

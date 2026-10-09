# Initial discovery — #reshape 16/19 `fn-sizes-rest`

Companion to [the changeset](2026-10-09-reshape-fn-sizes-rest.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — fn-sizes-rest (the functions over 60 lines outside `backends/`, and the seams `extract_method` can cut)

Measured on `master` at `4a5c42b1b` (2026-10-09) with the brace-matching scan of Exploration 3: from the line holding
`fn` to the closing brace, inclusive, production code only (skips `*_tests.rs`, `tests.rs` and everything after an inline
`#[cfg(test)] mod`). Thresholds from `.agents/commands/analyze-clean-code.md`: 41–60 needs attention, **>60 must refactor**.

### The list is nine functions, not eight

```
206 plan/codec.rs:246                                   parse_op
159 runner/entry_points/store_run.rs:237                apply_held_plan
115 runner/entry_points/check_entry_points.rs:207       check_plan
 95 crate_move/source_scan/sighting_walk.rs:33          sightings
 76 plan_store/refresh.rs:61                            refreshed
 73 restructure_args.rs:206                            options_for
 70 crate_move/cluster/stranded.rs:54                   stranded_siblings
 66 crate_move/cluster.rs:111                           resolve_cluster
 64 crate_move/source_scan/module_items.rs:32           items_of_module
```

The brief's "8 functions" is 9. The other 16 of the 25 are under `backends/` (node 19's, also not 17). The next one down
outside `backends/` is `verify.rs:166 compare_with` at 58 — in the deferred 41–60 band.

### What constrains a seam (the engine's rules, read from the code)

- **No `return` in the range** (`backends/rust/early_return.rs:33` `refuse_early_returns`): refused by `check`, `check
  --deep` and `apply`, naming the lines. **Exception:** a range that runs to the end of a value-returning function and ends
  with its tail expression (`runs_to_the_end_of_a_function`, `:165`). A `return` inside a closure in the range does not count.
- **`break`/`continue` leaving the range are not seen** (`docs/ft/coder/rust-code-restructuring.md` § Known limitations):
  only `apply`'s compile gate catches them. Every seam below stops before a `continue`/`break` of an outer loop.
- **Several extractions in one function compose bottom-up only**; the engine does not re-anchor a later op through an
  earlier op's edit. A seam whose range must *include* the call an earlier extraction wrote (parse_op's tail chain) needs
  its own plan, anchored after the earlier apply.
- **Arity.** `cargo clippy -D warnings` fails on 8+ parameters (`too_many_arguments`); node 4 (`extract-method-clean`)
  only *notes* arity > 7, it does not fix it. The crate has one justified `#[allow(clippy::too_many_arguments)]`
  (`runner.rs:66`, `commit_operation`). Every seam below is chosen with ≤ 7 inputs; counts are the locals the range reads
  or writes that are bound outside it.
- **A captured local closure** (sightings' `modules`) as an input: rust-analyzer cannot name a closure type, so the
  parameter is a placeholder. The engine's inferred-placeholder check refuses it (unverified on this exact shape — wave 2
  runs `check --deep` first).
- Node 4 makes the result keep its comments (P), write `&Path`/`&str`/`&[T]` (Q), import or respell the types its signature
  names (K) and accept a range starting on `{` or a comment (R). Without node 4, each of these extractions would need the
  hand fixes #524 recorded.

### Per function

**1. `plan/codec.rs:246 parse_op` — 206 lines (46 comment lines, 15 `return Err`).**
What it does: parses one plan line, refuses code-bearing fields (`:252-260`), the facade-repoint fields (`:265`),
deserializes, validates anchors, then runs **~20 cross-field validation rules** in sequence (`:286-447`): test-binary
facade/destination, which ops honour `reexport`, `move_item`/`reparent_module` destination and anchor kind, `outside`,
cross-crate named facade and destination, `also`, `to_file`, required `name`, `type`/`expr` syntax, then four calls into the
existing rule files (`signature_fields`, `canonical_paths`, `retarget_fields`, `repoint_call_fields`).
**It is not an op-name match.** The brief's "table dispatch candidate" does not fit: almost every rule spans several
kinds (`reexport` × kind, `moves_across_crates()`, `also` × `MoveClusterToCrate`), so a per-kind table would repeat each
cross-cutting rule in several rows or need a second, per-field table. The codebase already has the shape that fits: one
`refuse_…(op: &RefactorOp) -> Result<()>` per field theme in `plan/codec/*.rs` (e.g. `canonical_paths.rs:9`).
**Why plain `extract_method` cannot do it:** every guard is `if … { return Err(malformed(…)) }`; a range holding them is
refused as an early return. **What the engine can do today:** a range that runs to the function's tail `Ok(op)` is
exempt. So the guards can be cut as a **tail chain**, bottom-up, one plan each:

| Stage (bottom-up order) | Range (master lines) | New function (param `op: RefactorOp`, returns `Result<RefactorOp>`) | ≈ lines |
|---|---|---|---|
| 1 | `:420` `if op.name.is_none()` … `:450` `Ok(op)` | `checked_names_and_syntax` — `name`, `type`/`expr`, the four rule files | 33 |
| 2 | `:374` cross-crate named facade … `:419` + the stage-1 call | `checked_crate_moves` — named facade, `to`, `also` both ways, `to_file` | 48 |
| 3 | `:319` `if op.op == MoveItem` … `:373` + the stage-2 call | `checked_same_crate_moves` — `move_item`/`reparent_module` rules, `outside` | 57 |
| 4 | `:286` test-binary facade … `:318` + the stage-3 call | `checked_facades_and_test_binaries` | 36 |

`parse_op` keeps `:246-284` plus one tail call (≈ 40). Each stage ends by calling the next. All comments stay with their
rules (node 4's P carries them where rust-analyzer would drop them). The alternative designs are F1 in the PRD.

**2. `runner/entry_points/store_run.rs:237 apply_held_plan` — 159 lines.**
What it does: loads the held plan, builds the registry, opens the run (`open_plan_run`, `PlanRun` destructured into six
locals at `:258`), then in a closure (`:287`) loops the operations: stop limit, translate, resolve (`:305`), dry-run
accounting (`:322`, ends in `continue`), group enter, `commit_operation`, group settle (`:353-366`), plan refresh and report;
then rolls back a failed group and refuses a broken result (`:383`).
Engine seams with ≤ 7 inputs: held-plan lookup (`:245-251`, 2 inputs, −6), the resolve expression (`:305-313`, 5, −8), the
settle with its `on_check` closure (`:353-366`, 7, −13), `AppliedRun` + `refuse_a_broken_result` (`:383-390`, 7, −6).
**That reaches ≈ 126, not 60.** The loop's state (`journal`, `ledger`, `overlay`, `group`, `done`, `registry`, `paths`,
`legacy`, `plan`) is threaded through every phase: the dry-run phase reads 10 locals, the apply phase 11, the post-settle
refresh/report 10. No restructure operation introduces a parameter struct, and keeping `PlanRun` whole instead of
destructured is a hand edit too. This one needs a design decision (PRD F2).

**3. `runner/entry_points/check_entry_points.rs:207 check_plan` — 115 lines.**
What it does: verifies the snapshot, chooses the deep or static registry (`:217`), resolves item anchors or reports them
unresolvable (`:232`), plan-wide findings `unrunnable` + `stranded_siblings` (`:247-263`), a per-op static check (`:265-285`)
and, for deep runs, a rehearsal per op with its survey lines, notes and refusal (`:286-311`, after a `continue`), merges group
findings and prints the budget report (`:314-318`).
Seams (inputs): registry choice (`options`, `client`, `cancel`: 3, −9); item-anchor resolution (5, −8); plan-wide findings
(`root`, `ops`, `findings`: 3, −18); per-op static check returning `statically_sound` (7, −18); rehearsal and its account
from `:295` to the loop end (7 without the progress line, −18; with it, 8 — refused by clippy); budget report (3, −4).
≈ **40** after all six. No `return`; the `continue` at `:287` stays in `check_plan`.

**4. `crate_move/source_scan/sighting_walk.rs:33 sightings` — 95 lines.**
A token-level state machine: `frames`, `pending_test`, `brackets`, `module_block`, `at`, with `continue` in three arms.
Seams: the `modules` closure body (`:45-50`) → `enclosing_modules(frames: &[Frame])` (1 input, −4); the `{` arm body
(`:62-80`, 6 inputs, −17); the `use` arm (`:92-106`) and path arm (`:108-123`) bodies before their `at = …; continue;`
(6 and 7 inputs, ≈ −9 each). The last two read the `modules` closure → see the closure constraint above; if refused, the
function stays at ≈ 74 and the developer is asked (PRD F3). ≈ **56** if all four apply. Node 8 edits this function's
`pub(in …)` reading (`:107-121`); node 9 also reads it.

**5. `plan_store/refresh.rs:61 refreshed` — 76 lines.**
A `match` over anchor kinds. The `Item` arm holds two `let … else { return Ok(anchor) }` (refused as early returns). The
`Items` arm body after `let file = followed(file);` (`:113-133`) has no `return` (its `break` is inside its own loop) →
`refreshed_items(file, items, fingerprints, edited, resolver)` (5 inputs, −21). Optionally the `Anchor::Item` rebuild
(`:79-86`, 6 inputs, −6). ≈ **49–55**.

**6. `restructure_args.rs:206 options_for` — 73 lines.**
One `match`, one `Options { … }` literal per subcommand; the only logic is `Apply`'s `from`/`from_id` split. Seams: the
`Apply` arm expression (`:208-223`, 1 input `plan`, −14) and the `Anchors` arm block (`:236-245`, 1 input, −7). ≈ **52**.

**7. `crate_move/cluster/stranded.rs:54 stranded_siblings` — 70 lines.**
Loops the plan's moving modules; per module: paths naming the origin, whether the origin names it back (holds two
`continue`s, stays), where the named code is, and the finding text. Seams: the `whereabouts` match (`:88-104`, 3 inputs,
−16) and the finding's `format!` inside `findings.push((…))` (`:105-121`, 4 inputs, −14). ≈ **40**. Nodes 5 and 8 edit this
file (stranded children; reading every path).

**8. `crate_move/cluster.rs:111 resolve_cluster` — 66 lines.**
Per member: survey, header, refusals, merged edits (`:126-148`, five accumulators — any member-body seam needs ≥ 9 inputs).
Seams: the set-wide tail `:159` … `Ok(WorkspaceEdit …)` — runs to the end, `?` only, 6 inputs (−19); the two post-loop
absorb loops (`:150-155`, 4 inputs, −4). ≈ **43**. **The most-edited function on the list:** nodes 3, 5, 7, 8, 9 touch it,
and one of them changes its return type to a `Resolution` carrying a report; the seams must be re-read on the post-node-12
tree.

**9. `crate_move/source_scan/module_items.rs:32 items_of_module` — 64 lines.**
Top-level scanner: `mod`, `use`, defining keywords, `macro_rules!`. Seam: the `"mod"` arm body (`:54-71`, 4 inputs:
`scan`, `at`, `items`, `open_child`, −15). ≈ **49**. Node 11 reads its `use` leaves (aliases); node 5 its children.

### Collisions with wave-1/2 nodes (the line lands them before this node)

| Function | Nodes that edit it before node 16 | Expected effect |
|---|---|---|
| `parse_op` | 6 (`read_fields_through` rule call), 13 (`move_impl_members` rule call), 9 (`to` may name a new crate) | +1 call line each; stage 1 grows |
| `apply_held_plan` | 10 (op id in spawn records, stale-plan message), 3 (tidy reporting) | grows; re-measure |
| `check_plan` | 12 (item-anchor static verification at `:232-239`), 2/5/7/8 (`check --deep` parity, notes) | the anchor seam changes shape |
| `resolve_cluster` | 3, 5, 7, 8, 9 | return type and member body change |
| `stranded_siblings` | 5, 8 | finding text and path reading change |
| `sightings`, `items_of_module` | 8, 9, 11, 5 | small |

The binding rule ("put new logic in new functions") limits this, but nothing above is final until wave 2 re-measures.

### Verification: no function-length gate exists

- `/pr-wrap` step 3.5 and the code-issue records gate **file** length only (the `awk` counter of todo
  `2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md`, node 15's). `restructure check --budget`
  (`restructure_args.rs:121`, `check_plan :314`) also reports file lines.
- `tddy-code-analysis/src/complexity.rs` walks functions with `syn` and has `proc-macro2` `span-locations`, but records
  only the start line and cyclomatic complexity; `tddy-tools analyze` exposes no length.
- Precedent for a source-scanning test in this crate: `tests/library_returns_its_results.rs:213`
  (`only_the_command_line_front_end_writes_to_standard_output`, walks `src/` with `modules_under`).
- `syn` is a normal dependency of `tddy-code-restructuring` (`Cargo.toml:30`) but `proc-macro2`'s `span-locations` is
  not enabled for it; a scoped `./test -p tddy-code-restructuring` build does not unify features with
  `tddy-code-analysis`, so line numbers need a dev-dependency line (`proc-macro2 = { version = "1", features =
  ["span-locations"] }` — already in `Cargo.lock`, nothing new downloaded).
- Behaviour coverage that must stay green: parse_op's refusals are pinned in `src/plan.rs` `mod tests` (`:351`), `options_for`
  in `restructure_args.rs` tests (`:280`), the scanners in `crate_move/source_scan.rs` tests (`:327`), `check_plan` /
  `apply_held_plan` in `check_precondition_parity.rs`, `plan_store_acceptance.rs`, `transactional_groups_acceptance.rs`,
  `cluster_move*.rs` and the `store_run.rs` tests (`:472`).

### Addendum — after the developer's decisions (2026-10-09)

- **Guard lift moved into node 4.** A range whose every early exit is `return Err(..)`, in a function returning `Result`,
  is extracted as `-> Result<()>` ending `Ok(())` and called with `?`. `parse_op` is re-planned as seven guard lifts in
  today's order (`:250-260`, `:277-306`, `:308-315`, `:316-358`, `:360-394`, `:396-417`, `:418-436`) plus one plain
  extraction (`:437-444`), leaving `parse_op` at about 33 lines. The four-stage tail chain is no longer the plan.
- **`refreshed`'s `Item` arm is a tail-position seam (node 4 rule 13), not a guard lift.** Its two exits are
  `let … else { return Ok(anchor) }` (`plan_store/refresh.rs:88-96`). The range `:87-105` (`if edited.contains` to the
  arm's tail `Ok(anchor)`) has 7 inputs; the whole arm would have 9 (the `followed` closure and six destructured
  fields). With the `Items` arm, ≈ 40 lines.
- **F8/F10 on `parse_op`'s ranges:** all returns are `return Err(…)`; no top-level `let` in any range. `:316-358` and
  the merged last range `:418-444` also hold `?` calls.
- **Every target file binds `use crate::Result;`** (or `super::Result`): `plan/codec.rs:15`,
  `check_entry_points.rs:47`, `store_run.rs:3`, `refresh.rs:17`, `cluster.rs:6`, `stranded.rs:8`. A plain `?` extraction
  whose return type rust-analyzer writes as `Result<X, RestructureError>` is `E0107` there.
- **`apply_held_plan`:** consented hand edit (`HeldLoop` struct + `PlanRun` kept whole, `TODO(reshape-16)`), after which
  each phase has 7 inputs or fewer.

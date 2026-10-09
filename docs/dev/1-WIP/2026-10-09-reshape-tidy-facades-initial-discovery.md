# Initial discovery — #reshape 3/19 `tidy-facades`

Companion to [the changeset](2026-10-09-reshape-tidy-facades.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — tidy-facades (facade width and contents, declaration removal, the tidy's glob and gate-failure gaps)

Paths relative to `packages/tddy-code-restructuring/src/` unless noted. Checked against the tree at
`4a5c42b1b` (no engine commit since `480c84481`). Two behaviours were probed against the dev shell's
toolchain (rustc / rustfmt 1.8.0-stable) in a throw-away crate, outside the repository.

### 1. Glob facade width (`2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need`)

- **The width rule is already the one the entry asks for.** `backends/rust/facade.rs:124-130`
  `widest_visibility` returns `pub` when any surveyed item is written `pub`, else `pub(crate)`;
  `facade.rs:169` writes `{width} use {module}::*;`. A glob caps at each item's own visibility, so a
  width "from the consumers" cannot be wider than "the widest item" without re-exporting nothing
  (rustc's lint for a `pub` glob over no `pub` item, which `-D warnings` fails). Pinned today by
  `backends/rust.rs:3481` (`writes_one_glob_reexport_for_the_module_it_grouped`), `:4053`
  (`reexports_a_glob_at_pub_when_the_seam_moved_something_public`) and `:4066`.
- **So the two real failures came from the input, not the rule.** Both `#live-plan 10/15` splits
  (`51b8eb970`) moved items written `pub` (`journal/group.rs:13` `pub struct PreImage`, `:59`
  `pub struct OpenGroup`; `runner/entry_points/store_run/applied_op_record.rs:29` `pub fn
  record_applied_op`) and still got `pub(crate) use …::*`. `widest_visibility` can only say
  `pub(crate)` there if the survey listed none of them.
- **Most likely cause: the survey trusts an early-empty outline.** `survey_moved_items`
  (`backends/rust.rs:1650-1676`) and `survey_impl_members` (`:1750-1756`) read `documentSymbol`
  through `request_settled` (`:894-913`), which retries only on `ContentModified`. Every other outline
  reader — `module_outline` (`:1728-1729`), `item_move.rs:76`, `retarget_impl.rs:93`, `item_path.rs:271`
  — goes through `settled_outline` (`:1698-1719`), which does not believe an empty outline until the
  server has been seen to load (`outline_is_the_servers_answer`, `:1723-1725`; pinned at `:4351`,
  `:4366`). An empty survey is silent everywhere downstream: `refuse_stranded` has nothing to refuse,
  `facade_lines` writes `pub(crate)` for a glob and `empty_facade_note` (`facade.rs:255-262`) for a
  named facade, and `restore_visibility` restores nothing. The warm (index-daemon) path is the one the
  code issue `broken-restructure-anchors-empty-outline.md` already reports answering empty.
- **Ruled out:** item anchors lower to the item's full extent including docs and attributes
  (`item_anchor.rs:23`), so `covers` (`backends/rust.rs:2839-2846`) is not dropping documented items;
  `visibility_at` / `visibility_in` (`:2790-2825`) read `pub struct` as `pub`.
- **Not proven:** no reproduction exists. `fake_lsp` (`../tddy-lsp/tests/bin/fake_lsp.rs:522`,
  `:526`) answers `documentSymbol` from a fixed table and every `codeAction` with `ContentModified`, so
  it cannot drive `extract_module`; a live rust-analyzer cold run may well be green on master.

### 2. Named facade contents (`2026-10-03-…-leftovers-…` items 2 and 7)

- **Item 7 holds.** `facade.rs:170-206` keeps only `reached_from_outside && within.is_empty()`;
  `facade_will_bind` (`:136-149`) mirrors it, and `imports.rs:314` uses that to decide whether the
  parent needs its own import. An item written `pub` that nothing references leaves the public path.
  `MovedItem` (`backends/rust/seam_survey.rs:93-112`) carries `visibility`, so the rule needs no new
  survey data.
- **Item 2 holds.** `reach_of` (`backends/rust.rs:1833-1872`) sets `from_outside` for any reference in
  another file and for any same-file reference outside the relocated line range — including one inside
  the parent's own `#[cfg(test)] mod tests`. `MovedItem` has no production/test distinction.
  `runner/tidy/wide_facade_tests.rs` (fixture `tests/fixtures/tidy_wide_facade/`) is the tidy cleaning
  up exactly this after the fact (#539: 13 findings over 11 groups).
- Existing pins for named facades: `backends/rust.rs:3490` (grouped by visibility), `:3511` (only
  reached items), `:4863-4906` (nesting).

### 3. The tidy and globs (`2026-10-08-…-leaves-unused-reexports-and-an-orphan-doc-in-the-origin`, glob half)

- **The entry's diagnosis ("glob re-exports are skipped") is not what the code does.** rustc reports an
  unused `pub(crate) use m::*;` with a `MachineApplicable` whole-item removal (probed: `unused import:
  \`gone::*\``, span from the item's doc comment to the next line), and `unused_imports`
  (`runner/tidy.rs:412-439`) takes it like any other import. A glob nothing reads in any unit is removed.
- **What fails is a glob only the tests read.** The library unit reports it, the test unit does not, so
  it lands in `read_by_a_unit` (`tidy.rs:443-461`). Round one removes it (`reconcile`,
  `tidy/gating.rs:321-332`, gates nothing by design), the re-check fails on the test unit (`cannot find
  function \`used_by_tests\``), and `repair` (`tidy.rs:321-355`) matches failures to imports by
  `named_by_errors` (`gating.rs:67-76`) — which compares the quoted name to `bound_name`
  (`gating.rs:57-64`), `None` for a glob. Nothing matches, `repairs.matched.is_empty()` → `Repair::Failed`
  → the round is restored and **the run fails** with "the tidy was undone" over a tree that compiled.
- The repair already gates `read_by_a_unit` members — but only inside a statement some quoted name
  matched (`gating.rs:294-303`). A glob statement is never matched, so it is never reached.
- Gating (`gating.rs:163-185`, `rewrite`) already writes `#[cfg(test)]\n{head}use {path};` for an
  ungrouped statement, and `gateable_head` (`:126-145`) accepts `pub(crate)`; a glob needs no new
  rewrite, only to be matched.
- Side effect worth naming: `use std::fmt::Write;` read only by tests (leftovers item 5, second bullet;
  `docs/readiness-and-gates.md` § Known limitations) is the same shape — a library-unused import whose
  test-unit failure quotes a method, not the import's name — and is closed by the same rule.
- In the #536 R9 run the gate had also failed on other errors, so the tidy did not run at all (§ 5).

### 4. Declaration removal: orphan docs and rustfmt reorders

- **The orphan doc is the crate move's, not the tidy's.** `crate_move/manifest_edits.rs:6-19`
  `module_declaration` returns the span of the `mod x;` line alone; `crate_move/moving/facade_writer.rs:84-100`
  (`leaving`) replaces exactly that span with the facade or with nothing. Doc comments and attributes
  above the line stay and attach to the next item. One is on master now:
  `packages/tddy-session-lifecycle/src/lib.rs:101-103` — the doc of the moved `presenter_observer_task`
  sits on `pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};`. A stranded `#[cfg(…)]` would
  silently change the next item's meaning.
- The same-crate engine already reads declarations whole: `backends/rust/module_reparent/declaration.rs:66-118`
  `find` takes "the attributes and doc comments above it" via `attached_trivia_starts_at`
  (`backends/rust.rs:2426-2443`), with a pin at `declaration.rs:190`. `crate_move` cannot call either
  (`backends` depends on `crate_move` 30 times; the reverse would be a cycle).
- One path covers every crate move: `move_module_to_crate` goes through `cluster::travelling_alone`
  (`crate_move/cluster.rs:61`) into `resolve_cluster`, which calls `moving::left_behind`
  (`cluster.rs:150`) and `moving::declared_in_destination` (`cluster.rs:153`;
  `facade_writer.rs:291-313`, which inserts `pub mod {module};` via
  `manifest_edits::insert_module_declaration_sorted`, `manifest_edits.rs:28`).
- **The rustfmt entry's cause is wrong.** It says `lib.rs` "was not rustfmt-sorted before"; CI runs
  `cargo fmt --all -- --check` (`.github/workflows/ci.yml:73-74`), so it was. The squashed #536 diff of
  `tddy-session-lifecycle/src/lib.rs` shows the mechanism: `pub mod agent_list_mapping;` was the only
  line between two `use` runs; removing it joined them, and rustfmt sorts within a run. Probed with
  rustfmt 1.8.0: two `use` runs separated by a blank line are sorted independently; the same lines
  joined are sorted as one (doc comments travel with their item).
  `runner/tidy/format.rs:54-75` `format_touched` formats whole files — correct once the engine stops
  joining runs. Stable rustfmt has no line-range mode (`--file-lines` is unstable).
- Other removers that could join runs (`backends/rust/module_reparent`, `backends/rust/item_move`) were
  not observed doing it; out of scope here.

### 5. The tidy when the compile gate fails (`2026-10-08-…-miswrites-a-facade-path-…`, tidy half)

- `runner/compile_gate.rs:109-126`: `failing_check` runs first; on `Some(broken)` the tidy
  (`tidy_a_complete_run`, `:151-174`) is skipped and `AppliedTreeDoesNotCompile` (`lib.rs:229-243`)
  says nothing about it. Neither unused imports nor formatting happen. #536 R2/R3/R4/R7 commit messages
  each record a hand `cargo fmt` "because the engine's rustfmt pass does not run when its compile gate
  fails"; the host-block node recorded 17 `unused_imports` fixed with `cargo fix`.
- The gate's check is `--message-format short` (`compile_gate.rs:241-249`); its stderr already carries
  `file:line:col: warning: unused import: …` lines, so counting the unused imports in written files
  needs no second check.
- The tidy's whole design (`tidy.rs:1-32`) accepts a round only when the re-check compiles; running it
  on a broken tree is a different algorithm, not a flag.

### 6. Size and measurement

- `runner/tidy.rs` is **~540 production lines**, not 36: `runner/budget.rs:39-55` `production_lines`
  stops at the first `#[cfg(test)]` followed by a `mod` line, and `tidy.rs:37-38` is
  `#[cfg(test)] mod wide_facade_tests;` — an out-of-line declaration, not the test module. It is the
  only file in the crate this hides (scan of every production `.rs`). This node must not grow
  `tidy.rs`; its tidy changes belong in `runner/tidy/gating.rs` (479 lines total, tests included).
- `backends/rust/facade.rs` is 262 lines with no test module; its unit tests live in `backends/rust.rs`
  (`:3457-3530`, `:4053-4115`, `:4863-4910`), which node 17 carves. New facade tests go in a
  `backends/rust/facade_tests.rs` beside it, not in `rust.rs`.

### 7. Test harness

| Concern | Harness | Server |
|---|---|---|
| Facade lines, `facade_will_bind` | unit, `MovedItem` literals (`backends/rust.rs:3457-3479` `moved` / `moved_within`) | none |
| Empty-survey refusal | unit over the range text and `MovedItem`s | none |
| Survey reads a settled outline | live, `tests/extraction_defects_acceptance.rs` / `tests/import_pass_acceptance.rs` style (`harness::AFixtureWorkspace`), registered in `.config/rust-e2e.filterset:51` and the `rust-analyzer` group (`.config/nextest.toml:84-107`) | rust-analyzer |
| Glob gating | `runner/tidy.rs` tests' `a_crate_with` / `tidied_touching` (`tidy.rs:549-612`) — a real `cargo check` on a tempdir crate, no language server | none |
| Declaration span, doc travel, blank separator | unit in `crate_move/manifest_edits.rs` (tests at `:317-358`) and `crate_move/moving/facade_writer.rs`; end to end in `tests/move_facades_acceptance.rs` (live, `rust-e2e.filterset:60`) | none / rust-analyzer |
| Gate failure says the tidy did not run | `tests/apply_compile_gate_acceptance.rs:22` (`fails_an_apply_that_leaves_a_tree_the_compiler_rejects`) | none (test-binary move) |

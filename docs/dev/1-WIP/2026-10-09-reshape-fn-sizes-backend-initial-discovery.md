# Initial discovery — #reshape 19/19 `fn-sizes-backend`

Companion to [the changeset](2026-10-09-reshape-fn-sizes-backend.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — fn-sizes-backend (the functions over 60 lines in `backends/`, the seams `extract_method` can cut, and the claimed nesting record)

Measured on `master` at `4a5c42b1b` (2026-10-09) with the brace-matching scan of Exploration 3 (`fnlen.py`): from the line
holding `fn` to the closing brace, inclusive, production code only (skips `*_tests.rs`, `tests.rs` and everything after
an inline `#[cfg(test)] mod`). All paths below are under `packages/tddy-code-restructuring/src/backends/`. Line numbers
are master's; nodes 1–18 move them (node 17 moves every `rust.rs` function into `backends/rust/*`, node 15 moves
`visibilities` into `item_move/visibility.rs`), and wave 2 re-reads every seam on its base.

### The list is sixteen functions, not seventeen

```
163 rust.rs:1245                    resolve_opening      (impl RustBackend)
124 rust.rs:1520                    assisted_edit        (impl RustBackend)
 93 rust.rs:289                     assist_for           (free)
 91 rust/item_move/sites.rs:94      edits_for_file       (free)
 87 rust.rs:735                     start                (impl RustBackend)
 82 rust/item_move/assemble.rs:71   assemble             (free)
 73 rust/item_move.rs:55            move_items           (impl RustBackend)
 71 rust/item_move/assemble.rs:206  visibilities         (free)
 71 rust.rs:1022                    offered_assist       (impl RustBackend)
 70 rust/retarget_impl.rs:79        retarget_impl        (impl RustBackend)
 66 rust/visibility.rs:37           restore_visibility   (free)
 66 rust/item_move/sites.rs:324     rewrite_statement    (free)
 66 rust.rs:1124                    check                (impl LanguageBackend for RustBackend)
 65 rust/import_text.rs:18          choose_import        (free)
 64 rust.rs:823                     request              (impl RustBackend)
 62 rust/imports.rs:103             next_import          (impl RustBackend)
```

`{'>60': 16, '41-60': 45} total fns 703` under `backends/`; crate-wide `{'>60': 25, '41-60': 71}` (Exploration 3's
numbers hold: 9 outside `backends/` are node 16's, 16 here).

The brief's "17" is not on the code. The likeliest seventeenth is the function of the code issue this node claims,
`rust/facade.rs:151 facade_lines` — **58 lines**, under the cap, claimed for **nesting**, not length (below). Five
functions sit at **exactly 60** and are not over the cap: `rust/readiness.rs:147 await_answer`,
`rust/item_move/rebase.rs:99 path_edit`, `rust/item_move/assemble.rs:448 into_destination`,
`rust/item_move/assemble.rs:348 moved_text`, `rust.rs:1933 chain_module_to_file`. One more line from any wave-1–3 node
puts any of them on the list. The gate's output on the rebased base is the list, not this table.

### The claimed code issue measures nesting, and its measure is not the brace depth

`docs/code-issues/complexity-rust-facade-lines.md`: "**47 lines** · **nesting depth 5** … Thresholds breached: nesting
5 > 4"; the 2026-10-05 row reads 58 lines at HEAD and "brace depth 6 counting the function's own". A brace scan
(`tools/fnnest.py`, fn body = depth 1) reads **4** for `facade_lines` today: `match` (`facade.rs:156`) → `Named` arm
block (`:170`) → `.map(|tier| {` (`:192`) → `if tier.is_empty() {` (`:198`). The record's 5 is the indentation measure
`/analyze-clean-code` defines ("max levels of indentation in a function", `.agents/commands/analyze-clean-code.md:39`):
rustfmt indents the `.map(` chain one more level, so `String::new()` at `:199` sits 5 levels under the body. Under the
brace measure the record would close by redefinition, so the cut is planned against the record's own measure.

Crate-wide, the brace scan finds 8 functions nested past 4; in `backends/` two: `rust.rs:196 client_capabilities` (a
`json!` literal, data) and `rust/item_move/canonical_paths.rs:120 collected`. No gate measures nesting.

### What constrains a seam here (read from the code, in addition to node 16's list)

- **No `return` mid-range** (`rust/early_return.rs:33` `refuse_early_returns`), except a range that runs to the
  function's tail expression (`runs_to_the_end_of_a_function`, `:165`) in a function that does not return `()`
  (`ends_in_a_unit_tail`, `:66`). `?` is not a `return`. Node 4 adds the guard lift (a run of `return Err(..)` →
  `Result<()>` + `?`) and tail position one level down (a match arm or `if` branch in tail position).
- **Most `return`s here are value returns, not error guards.** `resolve_opening` is a dispatch: 12 of its 13 `return`s
  are `return Ok(…)` / `return self.<op>(op, workspace)` (`rust.rs:1286-1355`, `:1367`, `:1384`, `:1390`); only `:1251`
  is `return Err`. `check`'s five are `return <module>::findings(op, workspace)` (`rust.rs:1125-1139`) and one
  `return Ok(Vec::new())` (`:1141`). `assisted_edit` has one `return Ok(…)` (`:1604`) and no `return Err` (its refusals
  are `?`). So node 4's guard lift serves only `retarget_impl`'s `return Err(seam_refusal(…))` (`retarget_impl.rs:96-101`)
  here. **The cuts for the dispatchers are tail ranges**, today's exception.
- **A closure local cannot be a parameter.** rust-analyzer cannot name a closure's type, so the parameter is a `_`
  placeholder and `placeholder_checks::refuse_inferred_placeholder` refuses it (`rust.rs:1589-1594` path). This rules out
  every range of `edits_for_file` that reads `in_region` / `in_destination` (`item_move/sites.rs:108-116`) and every
  range of `rewrite_statement` that reads `refused` (`sites.rs:333-338`).
- **A returned reference with two reference inputs.** rust-analyzer writes no lifetimes; a new function returning `&str`
  / `Vec<&str>` / `Lost<'_>` with two reference parameters is `E0106`. Seams below avoid that shape
  (`next_import`'s `Lost` literal at `imports.rs:126-129`, `rewrite_statement`'s partition at `sites.rs:367-372`,
  `edits_for_file`'s qualifier at `sites.rs:103-107` are rejected for it).
- **Borrow splits.** `request`'s notification fold (`rust.rs:844-848`) reads `bridge` (a borrow of `self.bridge`) and
  writes `self.chatter`; as a `&mut self` method taking `bridge` it does not borrow-check. Not a seam.
- **A trait-impl method.** `check` is a member of `impl LanguageBackend for RustBackend` (`rust.rs:1104`); its seams use
  `self`. Where rust-analyzer puts a new `&mut self` method extracted from a trait impl is unverified: inside the trait
  impl it is `E0407`. Wave 2 runs `check --deep` on the first `check` seam first.
- **Arity.** 7 parameters or fewer, **counting the receiver** — conservative, and right after node 18
  (`backend-session`) turns the receiver into a session parameter. `/analyze-clean-code` calls more than 5 "must
  refactor" (`analyze-clean-code.md:45-49`); the seams below are ≤ 5 where the code allows and name the 6s and 7s.

### Every `?` extraction here names the crate's one-argument `Result` (resolved by node 4's rule 14)

Every file on the list imports `crate::Result<T>` (`lib.rs:261`; `rust.rs:20`, `item_move/sites.rs:21`,
`item_move/assemble.rs:31`, `item_move.rs:43`, `retarget_impl.rs:23`, `visibility.rs:10`, `import_text.rs:5`,
`imports.rs:16`, `facade.rs:10`). rust-analyzer writes a `?` range's return type as `Result<X, RestructureError>`, which is
`E0107` under that alias. Node 4's rule 14 respells the return type of every extracted function to the caller's
one-argument spelling (`extracted_fn/return_type.rs`), so the ≈ 30 `?`-bearing seams below need no hand fix.

### Per function (seams, inputs counting the receiver, size after)

**1. `rust.rs:1245 resolve_opening` — 163.** Unsupported-op guard (`:1250-1255`), the anchor's file read
(`:1257-1260`), text refusals before a server (`:1264-1278`, `?` only), then 13 op dispatch branches each ending in
`return` (`:1284-1373`), server start (`:1375-1377`), two more branches, the in-place assist and its `Resolution`
(`:1391-1406`). Seams: the text refusals → `refuse_before_a_server(op, original)` (2, −14); a **tail chain**,
bottom-up, one plan per stage, each range running to the function's tail (`Ok(Resolution { … })` or the previous
stage's call):

| Stage | Range (master) | New method (`&mut self`, `op`, `workspace`, `relative`, `uri`, `original` = 6) | ≈ lines |
|---|---|---|---|
| T1 | `:1375` `self.start(workspace.root)?;` … `:1406` | `assisted_resolution` — server start, multi-file assist, rename, in-place assist | 38 |
| T2 | `:1348` comment … `:1373` + T1 call | `text_resolution` — signature rewrites, `change_return_type` | 32 |
| T3 | `:1319` comment … `:1346` + T2 call | `crate_move_resolution` — cluster and test-binary moves | 35 |
| T4 | `:1280` comment … `:1317` + T3 call | `authored_resolution` — module move, `move_item`, `reparent_module`, `retarget_impl`, `repoint_call`, `repoint_facade_imports` | 46 |

Holding `return`s is legal for each: every range ends with the tail expression. `relative: String` and `uri: String`
pass by value along the chain (T1 moves `relative` into `FileEdit::Change`). `resolve_opening` keeps `:1245-1260` plus
two calls: **≈ 26**. Comments stay with their branches (node 4's P carries them where the range holds `?`).

**2. `rust.rs:1520 assisted_edit` — 124.** Seams: the borrow widening, a `let` initializer (`:1531-1537`, `op`,
`original`, `range` = 3, −6); the relocation surveys `:1550-1571` → `(moved, impl_members)` (`&mut self`, `uri`,
`original`, `range`, `relocates`, `reexport` = 6, −19); extract + name + rename + placeholder checks `:1573-1594` →
`(named, name)` (`&mut self`, `uri`, `original`, `range`, `op`, `impl_members`, `moved` = 7, −19); the import passes
`:1612-1618` → `(preserved, report, rerooted)` (`&mut self`, `uri`, `original`, `named`, `name`, `moved`, `reexport` =
7, −6; a 3-tuple, node 4 notes `type_complexity`); the facade tail `:1620-1642` (`preserved`, `name`, `impl_members`,
`report`, `moved`, `reexport`, `rerooted` = 7, no `self`, −21). The `!relocates` early return (`:1596-1605`) stays.
**≈ 52.** Node 4 replaces the `ExtractMethod` carry at `:1599-1603` with one call; re-measure.

**3. `rust.rs:289 assist_for` — 93.** A data table: eight arms of `Some(Assist { … })`, 12 lines each, no logic.
Seams: each arm's `Assist { … }` literal → a zero-argument function named for the assist (`extract_into_function`,
`extract_into_variable`, `extract_module`, `extract_module_to_file`, `generate_trait_from_impl`, `inline_into_all_callers`,
`remove_unused_parameter`, `convert_tuple_return_to_struct`), 0 inputs each, ≈ 14 lines each. **≈ 17.** Node 4 edits the
`ExtractMethod` placeholder's name (`:297`); node 6/13 may add a row (new rows go in new functions under the binding
rule).

**4. `rust/item_move/sites.rs:94 edits_for_file` — 91.** Derives `masked`, `statements`, `base`, `qualifier` and two
closures (`in_region` `:108`, `in_destination` `:109-116`) from its four parameters, then two loops: per `use`
statement (`:122-152`, three `continue`s of its own) and per remaining site (`:158-180`). Every loop-level seam reads a
closure (refused, above) or ≥ 8 locals: the statement loop reads 11, the site loop 10. Closure-free seams: the
`in_destination` body (`:110-115` → `in_destination_module(base, text, offset, context)`, 4, −5), the statement's sites
(`:123-127`, 2, −4), heads re-qualified (`:132-139`, 7, −6), the `covered` insert (`:144-146`, 3, −2), the facade-bound
condition (`:168-173`, 4, −4), the needed import (`:174-179`, 4, −5). **≈ 65 engine-only.** Same shape as node 16's
`apply_held_plan`: state threaded through both loops. PRD F2.

**5. `rust.rs:735 start` — 87.** Bridged branch (`:738-748`, two `return`s, stays), toolchain pinning `:758-771` →
`(toolchain, toolchain_bin)` (`&self` = 1, −12), the server command `:773-790` (`&self`, `root`, `toolchain`,
`toolchain_bin` = 4, −16), the handshake tail `:806-820` (`&mut self`, `root` = 2, −13). **≈ 46.**

**6. `rust/item_move/assemble.rs:71 assemble` — 82.** Seams: `facade_names` initializer `:82-89` (`moving`, `landing` =
2, −7); `moved_names` `:100-105` (1, −5); doc links `:118-127` (`moving`, `texts`, `region`, `edits` = 4, −8); the files
tail `:135-151` (`edits`, `texts`, `landing`, `notes` = 4, −15). **≈ 47.** Node 1 widens `Landing`'s fields read here;
node 15 moves `Moving` and `visibilities` out of the file.

**7. `rust/item_move.rs:55 move_items` — 73.** Seams: the `Resolution` tail `:112-126` (`assembled`, `destination` = 2,
−14); `moved` `:85-89` (`run` = 1, −4). **≈ 55.** (`:72-76` and `retarget_impl.rs:89-93` are the same five-line
server-open sequence; deduplicating it is not a function-size cut.)

**8. `rust/item_move/assemble.rs:206 visibilities` — 71** (moves to `item_move/visibility.rs` in node 15, body kept).
Seams: the users' widening `:229-238` (the right-hand side of `scope = …`; `moving`, `texts`, `region`, `item`,
`scope` = 5, −9); the reached items' loop `:257-274` (`moving`, `source`, `destination`, `landing` = 4, −17). **≈ 45.**

**9. `rust.rs:1022 offered_assist` — 71.** Seams: the target `:1030-1037` (`assist`, `range` = 2, −7); the code-action
request `:1053-1060` (`&mut self`, `uri`, `target`, `assist` = 4, −7); the inference probe `:1071-1075` (`&mut self`,
`assist`, `uri`, `probe` = 4, −4). **≈ 53.** The loop holds `return`s and is the function's tail, but its type is `!`
broken by `return`s, which rust-analyzer may print as the new function's return type; not used.

**10. `rust/retarget_impl.rs:79 retarget_impl` — 70.** Seams: the already-retargeted guard `:96-101` (guard lift, node
4: `run`, `new_name` = 2, −5); the moved text `:121-131` (`sites`, `file`, `layout`, `text`, `run`, `new_name` = 6,
−10); the `Resolution` tail `:134-147` (`workspace`, `file`, `to_type`, `text`, `replacement`, `layout` = 6, −13).
**≈ 42.**

**11. `rust/visibility.rs:37 restore_visibility` — 66** (29 comment lines). Seams: the kept widening's
`VisibilityChange` literal `:86-94` (`item` = 1, −8); the declaration index, the `let … else` initializer `:69-72`
(`source`, `block`, `item` = 3, −3). **≈ 55.** (The whole loop `:53-99` takes 7 and leaves a 50-line function; not
chosen.)

**12. `rust/item_move/sites.rs:324 rewrite_statement` — 66.** The `refused` closure (`:333-338`) is read by every
refusal, so ranges holding one are out. Seams: the regrouped tail `:377-388` (`head`, `prefix`, `kept`, `moved`,
`qualifier`, `drop`, `statement` = 7, no `return`, −11); the alias split `:343-347` (`tree` = 1, one reference in and
out, −4); the single-path replacement `:354-358` (`head`, `qualifier`, `name`, `alias`, `drop` = 5, −4). **≈ 47.**

**13. `rust.rs:1124 check` — 66.** Five value `return`s (`:1125-1139`), the range anchor's `let … else` (`:1140`), then
findings. Seam: the `extract_module` name claim `:1168-1186` (`&mut self`, `op`, `text`, `planned`, `findings` = 5,
−18). **≈ 48**; node 4 already replaces the `ExtractMethod` block `:1154-1166` with one call (≈ 37 after both).
Alternative: the tail `:1144-1188` (5 inputs, −43) — not chosen, it only moves 45 lines into one function.

**14. `rust/import_text.rs:18 choose_import` — 65** (31 comment lines). Three tiers, each ending in `return`. Seam: the
third tier as a tail, `:59` (its comment) … `:81` → `by_the_crate_binding_the_name(offered, in_scope)` (2, −22).
**≈ 43.** Risks: the range holds `?` on `Option` (`:80`) in a function returning `Option<&'a str>`, so rust-analyzer may
write `Option<Option<&'a str>>` and a `?` at the call; and the `'a` of the signature must be carried. `check --deep`
first; PRD F9.

**15. `rust.rs:823 request` — 64.** Seam: the narrated bridged request `:830-840` → `outcome` (`&self`, `bridge`,
`method`, `params`, `started` = 5, −9). **≈ 55.** The stdio loop `:860-885` is a tail range (−24) but has the `!` loop
type of `offered_assist`; not chosen.

**16. `rust/imports.rs:103 next_import` — 62.** The per-name loop returns from the function (`:136`, `:143`, `:157`) and
is not the tail. Seams: the aliased declaration `:132-134` and the bound declaration `:153-155`, each an
`Option<String>` (`inside`, `text`, `seam`, `name` = 4, −2 each). **≈ 58**, the thinnest margin on the list. The tail
`:113-163` (7 with `&mut self`) is the fallback; it leaves a 53-line function.

**17. `rust/facade.rs:151 facade_lines` — 58, nesting 5 (claimed record).** Seams: the `Named` arm body `:171-205` →
`named_facade_lines(module, items)` (2, holds `?`, −33); then inside it the tier line `:193-203` (the closure body;
`reached`, `tier`, `module` = 3). `facade_lines` ≈ 24 at nesting 2; the new function at nesting 2 by both measures.
Node 3 (`tidy-facades`) changes what the `Named` arm keeps (an unreferenced `pub` item, test-only names), so the arm is
re-read on the base.

### Collisions with the nodes below in the line

| Function(s) | Nodes that edit them first | Effect |
|---|---|---|
| all seven `rust.rs` functions | 17 (moves them, bodies unchanged per its E2.8: `resolve_opening`, `check` → `language_backend.rs`; `assisted_edit` → `extraction.rs`; `assist_for`, `offered_assist` → `assists.rs`; `start`, `request` → `transport.rs`; `chain_module_to_file` (60) → `assist_edits.rs`), 18 (turns `RustBackend` operations into free functions over a session handle) | new paths; `self` becomes a session parameter, counted in arity |
| `resolve_opening`, `assisted_edit`, `offered_assist`, `move_items`, `retarget_impl`, `next_import` | 18, **concurrent in wave 4**: detached to `fn …(session: &mut RustBackend, …)` (its P12, P8, P10, P4, P1, P3) | cut after node 18 (F5); helpers take `session` |
| `start`, `request`, `check` | 18 keeps them as methods (session module `transport.rs`; `LanguageBackend` member) | cut early with the free functions (F5) |
| `assisted_edit`, `check`, `resolve_opening` | 4 (one call each replaces the extract-method blocks; `refuse_unliftable_returns` at `:1271`) | shrink slightly |
| `assist_for` | 4 (placeholder name), 6, 13 (a row each if they add an assist) | rows |
| `assemble`, `visibilities`, `moved_text` (60), `into_destination` (60) | 1 (widening; `Landing` fields), 15 (moves `Moving`, `visibilities`) | moved; 60-line ones may cross |
| `edits_for_file`, `rewrite_statement` | 1, 11 (facade path in a moved impl block), 8 (grouped `use` split reuses Rule S) | re-read |
| `facade_lines` | 3 (named facade keeps unreferenced `pub` items, drops test-only names) | the `Named` arm changes |
| `resolve_opening`, `check` | every node adding an op (6 `read_fields_through`, 13 impl-member move) adds a branch | a branch each lands in T4/T3 or before the tail |

### Verification

- The gate is node 16's `tests/function_length_budget.rs` (syn with `proc-macro2` `span-locations`, consented there).
  Its test 1 skips `src/backends/` and its test 5 pins that skip; this node removes both.
- Line counts: each extraction adds a signature and braces (≈ +4 to +8 lines per seam, ≈ +200 in all). Production
  lines today: `item_move/sites.rs` 410, `imports.rs` 389, `import_text.rs` 318, `facade.rs` 262, `item_move.rs` 240,
  `visibility.rs` 209, `retarget_impl.rs` 195, `item_move/assemble.rs` 507 (→ ≈ 356 after node 15). Node 15's
  `engine_file_budget_shape.rs` caps every file at 500 once node 17 empties its exemption, so a file node 17 leaves near
  500 is re-measured before cutting into it. Node 17's new files: `language_backend.rs` 381, `assists.rs` 347,
  `transport.rs` 293, `extraction.rs` 231 — each has room for its seams (≈ +30 at most).
- The engine runs on its own crate: a `tddy-tools` and `tddy-index-daemon` built **before** the first cut, outside
  `target/` (`TDDY_INDEX_DAEMON_BIN`), so the tool applying the plans is not rebuilt from a half-cut tree.
- Behaviour oracle (unchanged, run after every apply): `move_item_*` (8 binaries), `retarget_impl_acceptance.rs`,
  `retarget_impl_plan_lines.rs`, `extract_method_*`, `extraction_defects_acceptance.rs`, `import_pass_acceptance.rs`,
  `move_facades_acceptance.rs`, `facade_cycle_acceptance.rs`, `relative_visibility_acceptance.rs`,
  `escaping_types_acceptance.rs`, `wait_heartbeat_acceptance.rs`, `wedged_request_acceptance.rs`,
  `index_health_acceptance.rs`, `check_precondition_parity.rs`, `cancellation_acceptance.rs`, and the unit tests in
  `item_move/sites.rs:411`, `retarget_impl.rs:196`, `imports.rs:398` and `rust.rs:2901+`.

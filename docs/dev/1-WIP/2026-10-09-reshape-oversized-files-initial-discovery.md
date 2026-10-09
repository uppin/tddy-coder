# Initial discovery — #reshape 15/19 `oversized-files`

Companion to [the changeset](2026-10-09-reshape-oversized-files.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — oversized-files (the production-line counter, the real oversized list, and the seams of each split)

Paths relative to `packages/tddy-code-restructuring/src/` unless noted. Checked against the tree at `4a5c42b1b`.
Measured with a throw-away script (kept beside the planning notes, not committed) that counts three ways:

- **awk** — the `/pr-wrap` step 3.5 gate (`.agents/commands/pr-wrap.md:121-128`): lines before the first line matching
  `^[[:space:]]*#\[cfg\(.*test[),]`, of any kind.
- **cut** — `check --budget` today (`runner/budget.rs:42-55` `production_lines`): lines before the first `#[cfg(test)]`
  whose next non-blank line starts `mod ` / `pub mod `.
- **items** — total lines minus the lines of every `#[cfg(test)]` / `#[cfg(all(test, …))]` item, attribute through the
  item's last line (`;` for a declaration, the matching `}` for a block; braces inside strings, chars and comments ignored).

### 1. Two counters, both wrong, in different ways

- **`check --budget`** (`runner/budget.rs:36-55`). The doc comment says the count ends at "the first `#[cfg(test)]` whose
  next non-blank line starts a `mod`" — which is true of an **out-of-line** `#[cfg(test)] mod x_tests;` as much as of the
  inline test module. So a file that declares an extracted test module near its top is cut there.
  `runner/tidy.rs:37-38` is `#[cfg(test)]\nmod wide_facade_tests;` → budget reports **36**; the file is **540** production
  lines (its inline `mod tests` opens at `:543`, the file is 1,034 lines). Pinned only by
  `budget.rs:292-304` (`does_not_end_early_at_a_test_only_import_above_the_module`, the `use` case) and `:306-317`;
  nothing pins the `mod x;` case. `production_lines` has exactly one consumer (`budget.rs:60`, `lines_of_file`); it is
  reached through `runner/entry_points/check_entry_points.rs:1-5`.
- **`/pr-wrap` step 3.5** (`.agents/commands/pr-wrap.md:121-128`) stops at the first `cfg(…test…)` of any kind — the
  todo `2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md` case (`connection_service.rs` measured 43).
  Its regex also matches `#[cfg(not(test))]` and `#[cfg(any(test, …))]`, which are not test-only.
- **Docs that restate the wrong rule:** `.agents/skills/code-restructuring/SKILL.md:17` ("before `#[cfg(test)] mod`"),
  `.agents/skills/deferred-work/references/code-issue-record.md:27` ("measured before the first `#[cfg(test)]`"),
  `pr-wrap.md:108-110`.
- **The engine already has one definition of "a cfg(test) item".** `crate_move/source_scan.rs:84-115`
  (`cfg_test_attribute`) recognises `#[cfg(test)]` and `#[cfg(all(test, …))]` over a masked token stream
  (`masked`, `:268`, built on `readable_spans`), and `source_scan/sighting_walk.rs:54` uses it. A syntax-aware count needs
  no new dependency: `syn` is a dependency (`Cargo.toml`) but reports no line numbers without proc-macro2's
  `span-locations` feature, and the token scanner already blanks strings and comments byte for byte.
- **Other counters in the repo** (not this node's): `tddy-daemon-rpc/tests/rpc_handlers_shape.rs:63`,
  `tddy-core/tests/core_facade_shape.rs:107`, `tddy-workflow-recipes/tests/module_shape.rs:17`,
  `tddy-daemon-kernel/tests/telegram_extraction_shape.rs:104`, `tddy-presenter/tests/presenter_split_shape.rs:116` — each
  its own heuristic.

### 2. The real oversized list (items rule), every production file of the crate

| File | items (real) | cut (`check --budget`) | awk (`/pr-wrap`) | total |
|---|---:|---:|---:|---:|
| `backends/rust.rs` (node 17's, excluded) | 2,959 | 3,008 | 510 | 5,566 |
| `crate_move/test_binary.rs` | **967** | 967 | 967 | 967 |
| `runner/tidy.rs` | **540** | 36 | 36 | 1,034 |
| `backends/rust/item_move/assemble.rs` | **507** | 507 | 507 | 507 |
| `backends/rust/item_path.rs` | 482 | 490 | 89 | 746 |
| `runner/tidy/gating.rs` | 479 | 479 | 479 | 479 |
| `plan/codec.rs` | 478 | 480 | 225 | 480 |
| `plan_store.rs` | 476 | 477 | 20 | 1,128 |
| `runner/entry_points/store_run.rs` | 471 | 471 | 471 | 718 |
| `journal.rs` | 460 | 460 | 460 | 1,043 |
| `item_anchor.rs` | 458 | 458 | 458 | 920 |
| `runner.rs` | 443 | 443 | 443 | 469 |

- **Three files over 500 besides `rust.rs`**, not two: the whole-work discovery's list (`rust.rs`, `test_binary.rs`,
  `assemble.rs`) missed `runner/tidy.rs`. `rust.rs` is 2,959 by the real rule (the cut over-reads it by 49 lines of
  `#[cfg(test)]` items above its test module).
- **The awk gate under-reads five files** by more than 300 lines (`rust.rs`, `tidy.rs`, `item_path.rs`, `plan_store.rs`,
  `header.rs` 2 vs 373); it is the blinder of the two.
- **Watch list (450–500)**: `item_path.rs`, `tidy/gating.rs`, `plan/codec.rs`, `plan_store.rs`, `store_run.rs`,
  `journal.rs`, `item_anchor.rs`. Wave-1 nodes add to some of them: node 3 (`tidy-facades`) adds `to_gate_in_a_repair` to
  `tidy/gating.rs` (479 today); nodes 6, 9, 12 add a `mod` and a call each to `plan/codec.rs` (478); node 12 edits
  `item_path.rs` and `item_anchor.rs`; nodes 2, 5, 9, 10 edit `store_run.rs`. This node's base is node 14's tip, so the
  list must be re-taken there with the fixed counter.
- No file under `src/` that is itself an out-of-line test module (`*_tests.rs`) is over 400; the largest is
  `runner/tidy/wide_facade_tests.rs` at 257.

### 3. `crate_move/test_binary.rs` (967) — four responsibilities

Item extents (doc comments and attributes included):

| Responsibility | Items | Lines |
|---|---|---:|
| The operation | `TestBinaryMove` + impl, `read_test_binary_move`, `test_binary_in_a_crate`, `resolve_test_binary_move`, `References`, `repointed_references`, `record_crates_declared_in_the_header`, `record_crates_the_code_names`, `Occurrence`, `origin_named_paths`, `is_a_whole_first_segment`, `written_path_from`, `record` (`:1-309`, `:511-584`, `:771-782`) | ~395 |
| Lexical masking — "is this byte code, a comment or a literal" | `Prose` (`:504`), `readable_spans` (`:591-643`), `block_comment_end`, `literal_end`, `raw_string_hashes`, `string_end`, `character_literal_end` (`:644-737`) | 154 |
| What a file names and binds | `is_a_built_in_root` (`:310`), `crate_shaped_heads`, `opens_a_path`, `inside_a_declaration`, `names_bound_in`, `behind_any_visibility`, `use_trees`, `record_names_bound_by`, `name_bound_by`, `grouped_members` (`:319-503`), `segment_length` (`:585`), `modules_declared_in`, `module_declared_by` (`:738-770`) | 233 |
| The facade walk | `Defining`, `defining_home`, `crate_past_the_facades` (`:783-873`) | 91 |
| Destination dev-dependencies | `destination_dev_dependencies`, `dev_dependency_line_for` (`:874-968`) | 95 |

- **The code issue's claim is wrong.** `oversized-file-test-binary.md` ("What would close it") says extracting the scanner
  "takes the file under budget on its own". The scanner it lists (masking + names/bindings) is 387 lines; 967 − 387 =
  **580**. Adding `origin_named_paths` and its helpers (74, which only `repointed_references` calls) gives 506 — still
  over. Under budget needs a third cut: the facade walk (→ ~489) or the dev-dependencies (→ ~485); both give ~394.
- **Consumers outside the file** (all `pub(crate)`, reached through `crate_move.rs:301-302`'s `mod test_binary; pub use
  test_binary::*;`):
  `crate_move/source_scan.rs:15` (`readable_spans`, `Prose`), `backends/rust/early_return.rs:6`
  (`crate::crate_move::{readable_spans, Prose}` — through the glob facade), `crate_move/header.rs:12` (`segment_length`),
  `crate_move/survey.rs:20` (`is_a_built_in_root`, `names_bound_in`).
- **`backends/rust/early_return.rs` depends on `crate_move` only for the masking.** Once the masking has its own module,
  the `backends → crate_move` edge from that file disappears — which the later engine-crate split
  (`2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md`) needs.
- **Hazard (unchanged):** the masking is the one definition of "code" for the re-pointing pass, the dependency
  collection, `source_scan`'s tokens and `early_return`'s range check. A move keeps it one definition; nothing copies it.
- No unit tests in the file; it is held by `tests/test_binary_move.rs` and `tests/apply_tidy_acceptance.rs`.
- Sizes of the candidate homes: `crate_move/manifest_edits.rs` 307 (+95 → 402), `crate_move/module_home.rs` 400
  (+91 → 491, no headroom), `crate_move/source_scan.rs` 326.

### 4. `runner/tidy.rs` (540) — the entry, and the import rounds

| Responsibility | Items | Lines |
|---|---|---:|
| Entry, check, report text | module doc (`:1-32`), `mod` lines, `Tidying`, `Tidied`, `tidy`, `Checked`, `Failure`, `check`, `errors_of`, `format_touched_files`, `report_remaining_warnings` | ~196 |
| The remove / redo / undo rounds | `Report` + impl (`:170-210`), `Round` + 2 impls, `Repairs`, `tidy_imports` (`:248-286`), `begin`, `Repair`, `MAX_REPAIRS`, `repair` (`:318-356`), `undone`, `restore` (`:369-399`) | ~230 |
| Choosing and applying rustc's fixes | `UnusedImports`, `unused_imports` (`:410-440`), `read_by_a_unit`, `apply_fixes` (`:463-485`), `refuse_unappliable` | ~114 |

- Private fields cross every candidate seam: `Report::accept` reads `Round`'s fields; `Round` holds an `UnusedImports`
  whose `fixes`/`primaries` `begin` and `accept` read. An engine move of either group needs the field and impl-member
  widening `move_item` does not do today (E0616 / E0624) — node 1 (`widen-same-crate`) delivers it.
- `runner/tidy/wide_facade_tests.rs` reaches `apply_fixes` through `use super::*` (`:4`); the inline tests call only
  `tidy(…)`. A move with `reexport: none` must re-point that test module too.
- Already children: `diagnostics.rs` 135, `format.rs` 278, `gating.rs` 479 (no inline tests). Node 3 adds no production
  line to `tidy.rs` (its changeset) but grows `gating.rs`.

### 5. `backends/rust/item_move/assemble.rs` (507) — the move's text, and the visibilities

| Responsibility | Items | Lines |
|---|---|---:|
| What a move is made of | `Moving` (`:33-55`) | 23 |
| The visibilities a move writes | `Landing` (`:174-188`, six private fields), `spelled`, `read_scope`, `visibilities` (`:206-277`), `users_of` (`:278-301`) | 128 |
| The assembly | `Assembled`, `written_from_the_root`, `assemble` (`:71-153`), `original_texts`, `repoint_callers`, `moved_text`, `moved_paths`, `named_from_the_root`, `leave_behind`, `into_destination` | ~356 |

- **Two cycles already pass through this file:** `item_move/reach.rs:13` uses `assemble::{users_of, Moving}` while
  `assemble.rs:23` uses `reach`; `item_move/canonical_paths.rs:16` uses `assemble::Moving` while `assemble.rs:14` uses
  `canonical_paths`. Giving `Moving` and `users_of` their own modules breaks both.
- `module_reparent/visibility.rs:19` already has a `Landing` for the same job in the other operation — the shape
  `item_move/visibility.rs` would mirror.
- `module_reparent/assemble.rs:10` reaches `item_move::assemble::written_from_the_root` (stays).
- The todo's first candidate (`doc_links`) is **already done**: `item_move/doc_links.rs` exists (190 lines); only its
  wiring (`moved_paths`, `named_from_the_root`) is left in `assemble.rs`.
- Node 1 edits this file (`Moving.reached: &Reach`, one changed call; member widening lands in a new
  `item_move/members.rs`) with no net line.
- Four functions here are on node 19's >60-line list (`assemble` 82, `visibilities` 71, `into_destination` 60,
  `moved_text` 60). A move keeps their bodies; node 19 re-anchors them.

### 6. Which operation each seam needs

- **Multi-consumer seams** (masking, names/bindings, `Moving`, visibilities) go to new modules whose callers must be
  re-pointed: `move_item` with `name` and `reexport: none` (SKILL.md § Gathering a topic module). An `extract_module`
  would leave a facade in the origin and keep `early_return → crate_move`.
- **Single-consumer seams** inside one file (the facade walk, the tidy rounds) can be `extract_module --to_file` children,
  several in one plan — node 2's projected tree is what makes such a plan's `check --deep` true.
- **Every plan here has several operations over files earlier operations created**, so node 2's projection applies to
  `move_item` plans too (it covers "every operation the Rust backend resolves through the server").
- **Field and impl-member widening** (node 1) is needed by the `tidy.rs` and `assemble.rs` seams. The brief lists only
  2 and 3 as real parents; 1 is a third.

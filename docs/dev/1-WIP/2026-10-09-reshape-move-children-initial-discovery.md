# Initial discovery — #reshape 5/19 `move-children`

Companion to [the changeset](2026-10-09-reshape-move-children.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — move-children (directory children, restricted `mod` lines, self facades, struct-update body paths)

Paths relative to `packages/tddy-code-restructuring/src/` unless noted. Read against `4a5c42b1b` (2026-10-09).

### Conclusions

- **A crate move renames exactly one file per member.** Nothing in `crate_move` reads a module's
  directory. The walker that would (`crate_move/module_files.rs`) already exists, is tested, and is used
  only by the same-crate operations. This is the root cause of all three "children left behind" entries.
- **Flattening is deliberate, not a slip.** `header.rs:47-48` documents "a nested member is declared at
  the destination's root under its own last segment", and `moving.rs:87-89` (`moved_to`) and
  `facade_writer.rs:291-313` (`declared_in_destination`) carry it out. It is right for a nested module
  that moves *without* its parent. It is wrong when an ancestor moves in the same set, which is the
  R3/R4/R7 shape. The self re-export is a side effect of the same rule.
- **The restricted-`mod` refusal is one strip-prefix.** `manifest_edits.rs:13` strips only `"pub "`.
  Five call sites inherit it.
- **The struct-update miss is in the tokenizer, not the survey.** `source_scan.rs:137-143`
  (`continues_a_path`) reads the second `.` of `..` as field access. So `..crate::a::f()` is never
  sighted. The same goes for `0..crate::MAX` and `..=crate::MAX`.
- **The dry-run count already equals the applied count.** The entry's "6 files, moved 2" compared a
  count of `FileEdit`s with a count of renames. See below.
- **`libc` is not this node's.** The origin declares it under `[target.'cfg(unix)'.dependencies]`.
  The manifest reader does not look at that table, so the survey drops the path before any manifest
  decision is made. That is node 9's slice.

### 1. Directory children (10-04, 10-08 strands, 10-08 also-members)

**What exists**

| Piece | Where | What it does today |
|---|---|---|
| `resolve_cluster` | `crate_move/cluster.rs:111-176` | For each member, one `FileEdit::Rename { from: member.source, to: member.moved_to() }` (`:133-136`). Its header pass, caller survey and cycle refusal all read `member.source` alone (`:127-131`). |
| `Move::of` | `crate_move/moving.rs:66-84` | `source = <crate>/src/<path>.rs` (`:80`). One file. |
| `Move::moved_to` | `crate_move/moving.rs:87-89` | `<dest>/src/<module>.rs`, which is the **last segment only**. This is the flattening. |
| `declared_in_destination` | `crate_move/moving/facade_writer.rs:291-313` | Writes `pub mod <module>;` into the destination root for **every** member, nested ones included. A carried child is therefore declared twice: at the root, and by its parent's own `mod` line. |
| `left_behind` / `leaving` | `facade_writer.rs:31-104` | Groups members by `home.declared_in` and replaces each member's `mod` line in that file with a facade (`:84-100`). For a child whose parent also moves, `declared_in` *is* the parent file. That file is renamed into the destination, so the facade lands there and names the destination: R3's `pub use tddy_session_files::session_attachment_materialization;`. |
| `parent_reexport_edits` | `facade_writer.rs:152-166` | With `reexport: none`, writes `<dest>::<module>` into the declaring file's `use <module>::*`/`{…}`. This is the second writer of a self-referencing path when the declaring file travels. |
| header rule for co-moving paths | `crate_move/header.rs:139-156` (`reach`) | `landing = member.rsplit("::").next()` (`:149`). So `crate::x::a::b::T` with member `x::a` lands as `crate::a::b::T`, which is correct for a carried tree. Prefix matching in `travels_with` (`:68-72`) already treats `a::b::…` as travelling with `a`. |
| `stays_behind_through_a_body` | `crate_move/preconditions.rs:85-120` | Surveys `moving.source` only (`:96-97`). A child's body reaching a module that stays behind is not seen. |
| stranded-sibling finding | `crate_move/cluster/stranded.rs:54-122`, `paths_naming_the_origin` `:128ff` | Reads `module.source` only. |
| `crates_still_naming_the_module` / `refuse_a_dependency_cycle` | `crate_move/refusals.rs:25-72` | Read the member's own header and rewrites only. |

**Reusable helpers (already tested)**

- `module_files::children_directory` (`module_files.rs:19-26`), `file_of_child` (`:30-44`) and
  `files_of` (`:51-62`). `files_of` walks `mod x;` and inline `mod a { mod b; }`. It refuses a declaration
  that leads to no file (`:77-86`). Tests: `:125-161`.
- Same-crate prior art that already carries children: `backends/rust/module_reparent/survey.rs:196-240`
  (`files_of_the_module`) plus `module_reparent/relocation.rs:32-54` (`plan`). `plan` maps each file to
  `<new_dir>/<name>/<inside>`. Its findings are "already exists where X would move to" and "places a
  module with `#[path]`" (`has_path_attribute`, `module_reparent/declaration.rs:141`). Its note is
  "`{n} file(s) move with the module, with the directory of its children`"
  (`module_reparent/assemble.rs:87-90`). `relocation::plan` is `pub(super)` inside `backends/rust`.
  `crate_move` cannot call it without inverting the dependency direction (`backends → crate_move`
  today), so a shared helper belongs in `crate_move/module_files.rs`.
- `Resolution.notes` (`edit.rs:71-75`) carries prose notes. `MoveClusterToCrate` currently returns
  `Resolution::of(...)` with none (`backends/rust.rs:1322-1331`).
- `source_scan::module_items::ChildModule.is_public` (`crate_move/source_scan/module_items.rs:13-21`)
  already tells `pub mod` apart from restricted or private declarations. It is useful for spotting a
  restricted child that something outside reaches.

**Why `check --deep` said `no findings`.** The static pass is `crate_move::unrunnable` plus
`stranded_siblings` (`runner/entry_points/check_entry_points.rs:254-262`). Neither opens a child file,
and `files_of` is never called on a crate-move path. The deep pass resolves through `resolve_cluster`,
which does not refuse: it moves the parent and leaves the children. The first failure is the apply's
compile gate (`E0583`).

**Parent-first then child (route 01c, `no parent module file`).** `module_home.rs:123-142`
(`parent_declaring_file`) checks `workspace.root.join(..).exists()` against the real disk, not the
overlay. So a static check of a later operation cannot see that an earlier operation moved the parent.
Once children are carried this route is no longer needed. The finding for a plan that names a child
the parent already carries is new work (see PRD).

**Not reproducible by reading: R7.** With nine children in `also`, nothing moved and the count was
`8 file(s)`. Today's code (`named_by` `cluster.rs:75-91`, `Move::of`, one rename per member) would
rename each child flat to the root, as in R3/R4, and give a count above 11. The original plan was not
kept. The red test for "`also` children nest" covers the shape either way.

### 2. Dry-run count vs apply (10-04 sub-item 2)

- Dry run: `runner/entry_points/store_run.rs:320`, `let files = resolved.edit.changes.len();`, printed
  via `progress_line(.., files, false)` (`:323-330`).
- Apply: `report_settled` (`store_run.rs:211-235`) prints `resolution.edit.changes.len()` with
  `applied = true` (`:223-230`). The format is `console.rs:273-287`, `"[{n}/{total}] op {i}: {kind} -> {files} file(s) {resolved|applied}"`.
- **Both count `FileEdit`s.** That is renames plus changed files: manifests, roots, callers. "6 files"
  was 2 renames and 4 changes. The third entry confirms parity (`resolved … 8 file(s)` then
  `8 file(s) applied`). The defect is that the line does not say how many files *move*. A carried
  child is a rename, so once children travel the count includes them. A note naming the moved files
  makes it unambiguous.

### 3. Restricted `mod` declarations (10-08 restricted, 10-08 hand-widened)

- `manifest_edits.rs:6-19` (`module_declaration`): `line.trim().strip_prefix("pub ")` then compares
  with `mod {module};`. `pub(crate) mod x;`, `pub(super) mod x;`, `pub(in crate::a) mod x;` and
  `pub(self) mod x;` never match.
- Callers that inherit the gap:
  - `preconditions.rs:197`: the refusal quoted in the entry, "declares no `mod x`".
  - `preconditions.rs:163`: `destination_already_has_the_module`. A destination declaring
    `pub(crate) mod x;` is **not** reported as a merge. This is a silent sibling of the same bug.
  - `facade_writer.rs:137` (`declaration_of`): the apply-side refusal.
  - `facade_writer.rs:227` (`is_declared_pub`): fine, because it wants `pub mod ` exactly.
  - `module_home.rs:196,239`: re-export following treats a restricted module of the origin root as
    "not declared".
  - Same gap, different function: `manifest_edits.rs:61-65` (`declared_module_name`), used by the
    sorted insertion. A restricted line is not seen as a neighbour.
- A visibility stripper that handles the restricted forms already exists in the same module tree:
  `facade_writer.rs:270-284` (`after_visibility`). Others: `backends/rust/module_text.rs:69-87`
  (`module_declaration` + `strip_visibility`, already restricted-aware), `verify/statements.rs:179`,
  `backends/rust.rs:2805` (`visibility_in`). That makes four copies. This node needs only
  `module_declaration` and `declared_module_name` fixed. Consolidating all four is out of scope.
- **Facade visibility.** Every facade is written `pub use`: `facade_lines_for_plan`
  (`crate_move.rs:346-368`) and `facade_line` (`crate_move.rs:313-337`). `written_facade`
  (`facade_writer.rs:184-223`) recognises an earlier facade only by the prefix `pub use {extern}::`.
  So a narrower facade would not be extended by a later operation. It would add a second line.
  Grouping is per destination (`crate_move.rs:347-356`) and has to become per (destination,
  visibility).
- Existing pins: `tests/move_facades_acceptance.rs:79-88,117-119` and `tests/move_paths_acceptance.rs:54,250`
  all start from `pub mod`, so mirroring the declaration's visibility does not change them.
  `cluster.rs` unit tests start from private `mod spawner;` (`cluster.rs:301`). They assert renames and
  headers, not facade text.

### 4. Struct-update body path (10-08 body paths, item 2)

- `crate_move/source_scan.rs:134-143`. `continues_a_path(at)` returns true when the previous token is
  `Kind::Punct(b'.')`. `..` tokenises as two `Punct(b'.')` (`Kind` doc, `source_scan.rs:27-35`), so
  `..crate::connection_service::starting_session_metadata(..)` is skipped as if it were `.crate`.
  `sighting_walk.rs:107` is the caller.
- Consequences: the path is never re-pointed (`header::rewrite_of`), and never reaches
  `stays_behind_through_a_body` (`preconditions.rs:100-106`), so neither apply nor check reports it.
  It is an `E0433` at the gate.
- The fix point is narrow: "behind `.`" should mean behind a **single** `.`, so not `..` and not `..=`.
  No other reader of `continues_a_path` relies on `..`.

### 5. Self-referencing re-exports (10-08 strands; 10-08 body paths, item 1)

- R3: the hand fix deleted the line. R4 and R8: `tddy_x::` → `crate::` (the child was flattened, so the
  re-export kept `parent::child` resolving). The writers are `leaving` (`facade_writer.rs:83-101`) and
  `parent_reexport_edits` (`:152-166`). Both run on the member's `declared_in` even when that file is
  in the travelling set (`cluster.rs:118`).
- With children carried at their nested position, the parent's own `mod child;` line and any
  `pub use child::…` stay valid **as written**. The right edit is none. A nested member whose parent
  does *not* move keeps today's behaviour (`header.rs:47`).

### 6. Closable as already done

- `docs/dev/todo/2026-09-09-macro-expansion-as-a-restructure-operation.md` is a research note whose
  recommendation is "do not build". Nothing to implement.
- `docs/dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md`: D6–D10 are marked
  fixed in the file. Spot checks: `refuse_mangled_rewrite`, `facade_will_bind`, `alias_target`,
  `widest_visibility` and the crate tier of `choose_import` all exist in `backends/rust*`. The "what a
  facade still cannot do" caveat is item M of `2026-09-24-restructure-apply-gaps-…` (node 2's file).

### 7. Not this node's (found while reading)

- **`libc` / target-specific tables (10-04 sub-item 3; 10-08 also-members `libc`).**
  `manifest_edits.rs:141-150` (`dependencies_of`) reads only `[dependencies]` / `[dev-dependencies]`.
  The origins declare `libc` under `[target.'cfg(unix)'.dependencies]` (`packages/tddy-daemon/Cargo.toml:122-123`,
  `packages/tddy-session-lifecycle/Cargo.toml:114-115`). So `survey.rs:87-89` drops `libc::…` as "not a
  crate". This belongs to node 9 (`new-crate`, "libc-style"). `async-trait` named only by an attribute
  is also node 9's.
- **Stranded doc comment** (10-04 "Routes tried" note): `module_declaration`'s span is the `mod` line
  alone, so a `///` above it stays behind. This is node 3's orphan-doc tidy.
- **A module in `a/mod.rs` shape cannot be anchored**: `module_home.rs:21-26` refuses a `mod` stem.
  This is out of scope and unclaimed.
- **`#[cfg(..)] mod x;` on one line** is not matched by `module_declaration` either (it does not start
  with `mod`/`pub`). Unclaimed.

### Test harness and fixtures to use

- Library level, no server: `resolve_cluster` and `MovingCluster` are public (`crate_move.rs:298-299`),
  and so is `ModuleReferences` (`crate_move.rs:73-84`). The `AKnownReferenceSet` fake
  (`cluster.rs:343-392`) and `a_workspace_with_an_entangled_pair` (`cluster.rs:286-317`) are the
  pattern. `cluster.rs` is already 921 lines (~260 production), so new unit tests go in a new
  integration binary `tests/crate_move_children.rs` with its own fake reference set. That keeps
  `cluster.rs` from growing.
- Static `check` parity: `tests/check_precondition_parity.rs` with `harness::a_workspace_holding_files`
  (`tests/harness/mod.rs:2643`) and `checking_the_plan` (`:2494`).
- `module_declaration` unit tests go in `manifest_edits.rs`'s test module. The struct-update sighting
  goes in `source_scan.rs`'s / `survey.rs`'s tests.
- Live, compiled: `tests/cluster_move_acceptance.rs` and `tests/move_module_to_crate_acceptance.rs`
  (`harness::performing` `:381`, `applying_a_plan_of` `:2452`, `assert_compiles` `:908`). Both are
  already in `.config/rust-e2e.filterset` (lines 46, 62) and the `rust-analyzer` nextest group, so a
  new case added there needs no registration.

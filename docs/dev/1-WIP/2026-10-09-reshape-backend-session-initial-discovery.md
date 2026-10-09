# Initial discovery — #reshape 18/19 `backend-session`

Companion to [the changeset](2026-10-09-reshape-backend-session.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — backend-session (what is inherent on `RustBackend`, what dispatches to it, and which engine route turns a method into a function)

Checked against the tree at `4a5c42b1b`. Paths are relative to `packages/tddy-code-restructuring/src/` unless noted. Node 17
(`feature/reshape/rust-backend-split`) moves most of `backends/rust.rs` into child modules before this node runs. Its PRD
was available when this was written; its changeset was not. So the inventory below is keyed by **member name** (stable
across node 17's moves), and each member's module after node 17 is taken from that PRD's layout table.

### 1. The inherent surface today: 79 members in 11 blocks over 10 files

Scanned with a brace walk over every `impl RustBackend {` block (production code only):

| File:line of the block | Members | Kind |
|---|---:|---|
| `backends/rust.rs:516-1093` | 23 | constructors and builders (`new` `:517`, `with_progress` `:547`, `with_cancellation` `:559`, `with_spawn_recorder` `:571`, `with_trace` `:577`, `with_wait_heartbeat` `:586`, `from_lsp_client` `:606`); transport (`workspace_root` `:642`, `take_id` `:651`, `keep_waiting` `:666`, `beat` `:690`, `waited_on` `:710`, `incomplete_index` `:725`, `start` `:735`, `request` `:823`, `request_settled` `:894`, `notify` `:915`, `send` `:922`, `receive` `:941`, `did_change` `:977`, `inference_ready_at` `:993`); the assist offer (`assist` `:1009`, `offered_assist` `:1022`) |
| `backends/rust.rs:1243-1483` | 3 | `resolve_opening` `:1245` (the dispatcher, 163 lines), `anchor_opening` `:1410`, `outside_references_opening` `:1449` |
| `backends/rust.rs:1485-2310` | 21 | `references_outside`, `assisted_edit`, `survey_moved_items`, `settled_outline`, `outline_is_the_servers_answer`, `module_outline`, `survey_impl_members`, `references_at`, `reach_of`, `prune_assist_imports`, `unresolved_names`, `chain_module_to_file`, `edit_for`, `multi_file_assist`, `rewrite_signature` (no receiver), `wrap_or_unwrap_return_type`, `anchor_range`, `rename_symbol`, `locate_symbol`, `extract`, `rename_placeholder` |
| `backends/rust/documents.rs:23-59` | 3 | `did_open`, `closing_what_it_opens<T>` (generic over the entry point), `close_opened` |
| `backends/rust/readiness.rs:42-231` | 7 | `ensure_indexed`, `wait_until_resolved`, `wait_until_answerable`, `wait_until_resolved_within_bound`, `await_answer`, `pull_diagnostics`, `refuse_degraded_index` |
| `backends/rust/imports.rs:18-296` | 7 | `restore_imports` and its six helpers |
| `backends/rust/item_move.rs:53-226` | 4 | `move_items`, `range_of`, `sites_of`, `reached_by_the_moved_code` |
| `backends/rust/item_path.rs:257-317` | 3 | `outline_of`, `resolve_item_opening`, `item_enclosing_opening` |
| `backends/rust/module_reparent.rs:43-112` | 2 | `reparent_module`, `callers_of_the_module` |
| `backends/rust/retarget_impl.rs:77-162` | 2 | `retarget_impl`, `retarget_range` |
| `backends/rust/signature.rs:81-153` | 2 | `name_converted_struct`, `rename_introduced` |
| `backends/rust/repoint_call.rs:48-84` | 1 | `repoint_call` |
| `backends/rust/repoint_facade.rs:45-55` | 1 | `repoint_facade_imports` |

Trait impls on the type: `Drop` (`rust.rs:1095`), `LanguageBackend` (`rust.rs:1104-1226`, 9 members), `ModuleReferences`
(`rust.rs:1233-1241`), `ItemResolver` (`item_path.rs:334`), `ItemAtResolver` (`item_path.rs:340`).

**The set grows before this node runs.** Lower nodes add inherent members: node 1 `reached_by_the_move`,
`reached_by_the_tree`; node 2 `stage_projection`, `open_staged_projection`; node 4 `cleaned_extraction`,
`verified_narrowings`, `respelled_signature_types`; node 6 `read_fields_through`; node 13 `move_impl_members` (each named
in that node's draft contract). So the inventory has to be retaken at this node's base, and the end state has to be pinned
by a rule, not by a list.

### 2. One operation already has the target shape

`backends/rust/repoint_call/sites.rs:29-33` is already a free function over the state:
`pub(super) fn repoint_receivers(backend: &mut RustBackend, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution>`.
Its body calls `backend.start` / `backend.did_open` / `backend.ensure_indexed` / `backend.sites_of` (`:54-59`), and
`repoint_call` reaches it with `sites::repoint_receivers(self, op, workspace)` (`repoint_call.rs:70`). So "the existing
state behind `&mut`" already works as a session handle in this crate, with no new type.

### 3. The transport core calls no operation

The session state (`rust.rs:425-490`: `server`, `bridge`, `next_id`, `doc_version`, `opened`, `indexed`, `chatter`,
`cancel`, `unresolved_token`, …) is read and written by the transport block, `documents.rs` and `readiness.rs`. A scan of
those bodies for calls to any other inherent member finds **one**: `assist` calls `offered_assist` (`rust.rs:1018`), both
of them assist-offer code, not transport. `readiness.rs` calls only `request_settled`, `workspace_root`, `keep_waiting`,
`incomplete_index`, `refuse_degraded_index`, `wait_until_answerable`, `await_answer`, `pull_diagnostics`. So "session core"
is a real seam: the core reaches nothing above it. Node 17's PRD draws the same line. It names `transport`, `readiness`,
`documents`, `references` (plus `handshake` and `lsp_edits`, which never name the type) as the session modules, and it adds
the shape check "the session modules reach no operation module".

Operations read session **fields** directly in about 15 places: `(self.progress)(…)` in 11 members (`resolve_opening`,
`anchor_opening`, `assisted_edit`, `references_at`, `multi_file_assist`, `wrap_or_unwrap_return_type`, `rename_symbol`,
`callers_of_the_module`, `move_items`, …), `self.trace` (`survey_impl_members`), `self.indexed` / `self.chatter` /
`self.environment` / `self.wait_heartbeat` (`offered_assist`, `outline_is_the_servers_answer`, `locate_symbol`,
`settled_outline`), and `self.unresolved_token` (`unresolved_names`). As free functions in child modules of
`backends::rust`, they can still read them. A private field is visible in the defining module's descendants. Across
crates (stack 2) it cannot.

### 4. Dispatch: the registry and the runner reach the backend through one trait object

- `registry.rs:111-114`: `BackendRegistry { backends: Vec<Box<dyn LanguageBackend>> }`; `register` (`:121`),
  `backend_for` / `backend_of` (`:131`, `:146`) return `&mut dyn LanguageBackend`. Item anchors go through
  `backend.item_resolver()` / `item_locator()` (`:170-198`).
- `runner/entry_points.rs:8,95,133`: the only concrete use of `RustBackend` in `runner/`, all of them construction
  (`RustBackend::new(…)` boxed for static checks; `from_lsp_client(…).with_wait_heartbeat(…)` plus `with_trace`). The rest of
  `runner/` imports only `ProgressSink`, `discard`, `WAIT_HEARTBEAT`, `human_delta` (`runner/options.rs:8`,
  `compile_gate.rs:33`, `tidy.rs:46`, `group_gate.rs:21`, …).
- Tests construct the type and call trait methods on it: `tests/harness/mod.rs:435,486,2577`,
  `tests/cancellation_acceptance.rs:57,217`, `tests/facade_imports/mod.rs:212,235`, `tests/spawn_record_acceptance.rs:397`,
  `tests/workspace_root_acceptance.rs:167`, `tests/wait_heartbeat_acceptance.rs:104`.
- Inside the trait impl, every entry point goes through `closing_what_it_opens(|backend| backend.<x>_opening(…))`
  (`rust.rs:1204,1208,1239`; `item_path.rs:336,342`). `closing_what_it_opens` takes `impl FnOnce(&mut Self) -> Result<T>`
  (`documents.rs:39-47`), so a closure that calls a free function `x_opening(backend, …)` type-checks unchanged.
  `module_references` / `item_resolver` / `item_locator` return `Some(self)` (`rust.rs:1216,1220,1224`).
- Operations hand the state on as a trait object in two places: `crate_move::resolve(self, workspace, op)`
  (`rust.rs:1286`, into `engine: &mut dyn ModuleReferences`, `crate_move.rs:180-184`; cluster `crate_move/cluster.rs:111-115`)
  and `span_of(&op.anchor, workspace.root, self)` (`item_move.rs:137`, `retarget_impl.rs:158`, as `&mut dyn ItemResolver`).
  Both coerce from `&mut RustBackend` as well as from `self`.

**Consequence for stack 2 (the brief's premise is incomplete).** `impl LanguageBackend for RustBackend` can only be written
in the crate of `RustBackend` or of `LanguageBackend` (orphan rule, E0117/E0116). Its `resolve` reaches every operation
through `resolve_opening` (`rust.rs:1245-1408`: `crate_move::resolve`, `self.move_items`, `self.reparent_module`,
`self.retarget_impl`, `self.repoint_call`, `self.repoint_facade_imports`, `resolve_cluster`, the assists, …). The model
crate that owns the trait sits at the bottom of the proposed split
(`docs/dev/todo/2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md`, crate 1). So whatever crate
holds `RustBackend` also holds the dispatcher, and the dispatcher depends on every operation crate. Free functions are
**necessary** for the split, but they do not make it possible. One more seam is needed, either a type split (a
`RustBackend` in the top crate wrapping a session type in the bottom Rust crate) or a dispatch table. This node does not
build it (decision F3). It writes the todo.

### 5. Name of the session parameter

Node 6's self mode refuses a binding name already written in the method's body, as a whole word in masked code (its RS4).
`backend` is written in `resolve_opening` as a struct-field initializer, `backend: "Rust".to_string()` (`rust.rs:1252`),
and the trait impl's closures name `|backend|`. `session` occurs in no body of the members to convert. Its only
occurrences in `backends/` are a string in `start` (`rust.rs:736`), which is a transport member that stays, and strings in
inline tests (`rust.rs:4189-4773`). The existing free function's parameter is `backend` (`repoint_call/sites.rs:30`).

### 6. Hazards in the bodies to convert

- `Self` in a body: one, `Self::rewrite_signature(op, &original)` (`rust.rs:1351`), the only call of an associated function
  without a receiver (`rewrite_signature` `rust.rs:2066`). A free function cannot name `Self` (E0411). Node 6 leaves `Self`
  untouched (its limits).
- Early `return`s: `resolve_opening` has 13, `offered_assist`, `locate_symbol`, `multi_file_assist`, `wrap_or_unwrap_return_type`
  and others have 1–3. Every body returns `Result<…>` or `bool`, never `()`.
- Receivers: every call site of these members is on `self` or on the closure parameter `backend`, and both have type
  `&mut RustBackend` (§ 4). No call sits inside a macro invocation (`grep '[a-z_]!(.*\(self\|backend\)\.[a-z_]*('` finds only
  an inline-test `assert!` on `keep_waiting`, a transport member).
- Generics: none of the members to convert is generic. The only generic inherent member is `closing_what_it_opens<T>`,
  which is transport.
- By-value `self`: only the builders (`with_*`, `rust.rs:547-596`), which stay.
- Free-name clashes: no module that holds a member to convert already declares or imports a free item of the same name
  (checked per file for `fn`/`use`/`mod` of each name).

### 7. Engine routes from "method" to "function plus callers", and what each leaves

Callers today: 1 to 15 per member, about 70 in all (`self.<m>(` / `backend.<m>(` counted per file). A route has to do three
things: write the body as a function over the state, rewrite every caller, and leave no method behind. A forwarding
method left on the type keeps the dependency from the type's crate up to the operation, which is the very edge stack 2 has
to cut.

| Route | Body | Callers | Method left | Evidence |
|---|---|---|---|---|
| Node 6 self mode (`read_fields_through` with `"expr":"self"`), then `extract_method` | `let session = self;` plus the rebound body, then a free `fn m(session: &mut RustBackend, …)` (node 6's test 13 pins the free function) | untouched: they still call the method | `fn m(&mut self, …) { let session = self; m(session, …) }` | node 6 changeset, Successor PRs and test 13 |
| … then `inline_method` on the forwarder | — | rust-analyzer's "inline into all callers" substitutes an argument directly only when it is a local name, a literal or a closure, and binds every other argument with `let` (its `inline_call` handler), so `self.settled_outline(&uri)` becomes a block. The forwarder's own `let session = self;` is copied into each caller, where it **moves** `self` (`&mut` has no implicit reborrow in an unannotated `let`) and breaks later uses (E0382), or writes `let session = session;` (clippy `redundant_locals`) | removed, but an emptied `impl RustBackend {}` stays, and a private extracted function named from another module is E0603 | `inline_method` has **no live test** in `tests/` (grep); `plan-schema.md:651` documents only the macro-call limit |
| … then per call site `repoint_call` (single form, `self.m` → `m`) and `add_call_arg` (`self`, `first`) | — | exact: `repoint_call/single.rs:14-28` keeps the arguments byte for byte | the forwarder stays, now dead. **No operation deletes an item.** Deleting it by hand would be a build fix only because of `dead_code` under `-D warnings`. The empty block is not even a warning | `refactor_kind.rs` (no delete op) |
| A new authored operation that removes the member, writes the function and rewrites callers from the server's reference set | `self` → `session`, `Self` → the type (node 6's self-mode text functions) | `R.m(a)` → `m(R, a)` / `M::m(R, a)`, `Self::m(a)` → `m(a)`, from `sites_of` (`item_move.rs:144`), classified as `repoint_call`'s bulk form already does (`repoint_call/sites.rs:118` `classify_sites`, `:197` `is_a_method_call`) | none; the emptied block goes as node 13's does | reuse: `retarget_impl/outline.rs:56` `read` (members of one block, attached trivia), `retarget_impl.rs:185` `member_text`, `item_move/text.rs:209` `use_insertion` |

Visibility needs no widening on the last route. An inherent method's visibility is relative to the module of its `impl`
block, and a free function written in the same module with the same visibility reaches exactly the same callers.

### 8. Interactions with the wave-4 sibling and with node 17's checks

- Node 19 (`feature/reshape/fn-sizes-backend`, next in the line) lists nine `RustBackend` methods "that node 18 rewrites":
  `resolve_opening`, `assisted_edit`, `start`, `move_items`, `offered_assist`, `retarget_impl`, `check`, `request`,
  `next_import`. Under the session-module rule (F4), `start` and `request` are transport and stay methods, and `check` is
  a `LanguageBackend` member that stays. Node 19 can cut those three without waiting. If node 19 extracts new `&mut self`
  helpers out of `start` or `request`, they land in `transport.rs`, which the rule allows.
- A body rewritten from `self` to `session` gets 3 characters longer at each site. rustfmt can rewrap a line that crosses
  100 columns, so a function on node 19's list can gain a line without gaining logic.
- Node 17's `tests/engine_module_edges_shape.rs` adds "`rust.rs` holds one inherent `impl` and no trait impl" and "the session
  modules reach no operation module". This node relies on both and does not duplicate them.

### 9. Test harness

- Live fixtures: `tests/same_crate/mod.rs:32` `an_app_holding` (one crate), `tests/harness/mod.rs:916`
  `assert_compiles_with_its_tests`, `:2673` `assert_lints_clean`. A live binary is registered in `.config/nextest.toml`
  (the `rust-analyzer` group) and `.config/rust-e2e.filterset`.
- Library level: plan lines through `RestructurePlan::parse` and `RustBackend::check`, as `tests/retarget_impl_plan_lines.rs`
  does; `verify` through `verify::compare_with`, as `tests/verify_accounts_for_a_retarget.rs` does.
- Shape checks over source text, as `tests/engine_module_edges_shape.rs` (node 15, extended by node 17) does.
- `verify` carriers for a new declaration are the ones node 6 extends with `rebinds`: `restructure_args.rs:249-250`,
  `runner/options.rs:87-90,141-142,206-214`, `runner/comparison.rs:31`, `tddy-index-daemon/proto/code_index.proto:280-288`
  (field 5 is node 6's, so 6 is next), `tddy-index-daemon/src/cli.rs:387,685-713`, `tddy-index-daemon/src/queries.rs:290`,
  `tddy-tools/src/index_client.rs:375`.

### 10. Reconciliation with node 17's changeset (added 2026-10-09)

Node 17's changeset (State B, plans A–K) confirms the session line: `workspace_root`/`take_id` and `start` … `did_change` go
to `transport.rs`; `keep_waiting`/`beat`/`waited_on`/`incomplete_index` join `readiness.rs`; `settled_outline` and
`outline_is_the_servers_answer` join `documents.rs`; `references_outside`/`references_at` go to `references.rs`; and its
shape check forbids the session modules (`handshake`, `transport`, `readiness`, `documents`, `references`, `lsp_edits`)
from naming any operation module. Differences from the PRD-based layout above:
- `inference_ready_at` goes to `assists.rs` with `assist`/`offered_assist`, its only caller (`rust.rs:1072`), so it is an
  operation here, not session.
- `unresolved_names` goes to `extraction.rs` (callers `rust.rs:1891`, `imports.rs:58,291`); detached.
- `anchor_opening` and `module_outline` are not in node 17's plans: node 12 deletes them (with `anchor_for`) first.
- Block #1 carries node 10's `with_silence_bounds`, a by-value builder: construction surface.
- Node 4's three members live in its new `extracted_fn.rs`, not in `extraction.rs`.
- Node 2's `projection.rs` holds `stage_projection`/`open_staged_projection`, which `documents.rs`'s `did_open` uses for
  staging; node 17's session must-not list does not name `projection`. It is treated as a session module, so a session
  module never reaches an operation module.

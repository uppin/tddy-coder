# Initial discovery — #reshape 9/19 `new-crate`

Companion to [the changeset](2026-10-09-reshape-new-crate.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — new-crate (creating the destination crate; manifest completeness)

Paths relative to `packages/tddy-code-restructuring/src/` unless noted. Verified against the tree at
`4a5c42b1b` (no engine commit since `480c84481`, #595).

### 1. A missing destination is refused, by design, in one place every crate move reads

- `crate_move/destination.rs:28-35` — `Destination::read(root, dir)` reads `<dir>/Cargo.toml` with
  `std::fs::read_to_string` and returns `MalformedPlan("`{dir}` is not a crate: … could not be read")`.
  The doc comment (`:26-27`) states the rule this node reverses: *"a plan that names a crate which does
  not exist is a plan defect, and scaffolding a crate is authoring, not moving."*
- The rule is pinned by `crate_move.rs:600-617` `refuses_a_destination_with_no_manifest` (asserts the
  refusal names the directory and `Cargo.toml`). That test changes, it is not deleted: the refusal stays
  for a plan that does not ask for creation.
- Callers of `Destination::read` for the **destination**: `crate_move/moving.rs:53` (`Move::read`, single
  module), `crate_move/cluster.rs:88` (`named_by`, cluster), `crate_move/test_binary.rs:59` (test binary),
  `crate_move/cluster/stranded.rs:191-192` (static stranded-sibling finding — `.ok()?`, so a missing
  destination silently drops the op from that finding).
- **`Destination::read` bypasses the overlay.** It takes `root: &Path` and reads disk, while every other
  read in a resolution goes through `Workspace::read` → `Overlay::read` (`registry.rs:32-34`,
  `overlay.rs:33-38`). `check --deep` (`runner/rehearsal.rs:64-65`) and `apply --dry-run`
  (`runner/entry_points/store_run.rs:331-332`) fold each op's edit into an overlay and never touch disk;
  `apply` writes each op (`runner.rs:83`). So a second op moving into a crate an earlier op of the same
  plan created would resolve under `apply` and be refused under `check --deep`/`--dry-run` — a parity
  gap that does not exist today only because nothing creates a `Cargo.toml` yet.
- **The static `check` is per-op against the original tree.** `crate_move/preconditions.rs:49-66`
  `unrunnable` runs `move_preconditions` (`:189-214`) → `Move::read` → `Destination::read` for every
  cross-crate op with no accumulated overlay; ordering across ops is modelled only for *moved modules*
  (`moved_by_earlier_operations`). A destination created by an earlier op needs the same treatment.
- `crate_move/preconditions.rs:156-175` `destination_already_has_the_module` reads the destination root
  (`workspace.read(&root)?` — refuses when it cannot be read) and checks `workspace.root.join(&target).exists()`
  on disk. For a crate being created both must answer "empty / absent", not error.

### 2. Most of the "new crate" plumbing already exists

- **Workspace members:** `crate_move/moving.rs:250-273` `workspace_members` already appends
  `"<dest dir>",` at the end of the root `members` array when it is missing (via
  `manifest_edits::members_list`, `manifest_edits.rs:199-204`), skips when no root manifest or no
  `members` array. Pinned by `crate_move.rs:862-881` `adds_the_destination_crate_to_the_workspace_members`.
  Added once per cluster (`cluster.rs:174-176`). Appends, does not sort.
- **Destination manifest dependencies:** `crate_move/moving.rs:117-204` `destination_manifest` /
  `dependency_lines` — copies each named crate's line verbatim from the origin's manifest
  (`manifest_edits::dependency_line`, re-anchoring a relative `path` via `re_anchored`, `manifest_edits.rs:211-222`),
  authors only the path back to the origin, refuses a crate the origin does not declare
  (`moving.rs:190-196`) and asserts the destination is not in its own set (`:123-131`).
  `with_dependencies` (`manifest_edits.rs:153-169`) appends the table at the end when absent — so it works
  on a skeleton manifest with no `[dependencies]` table.
- **Module declaration:** `moving/facade_writer.rs:291-314` `declared_in_destination` inserts
  `pub mod <m>;` sorted into `<dest>/src/lib.rs` (`Move::destination_root`, `moving.rs:91-94`);
  `insert_module_declaration_sorted` (`manifest_edits.rs:28-58`) handles an empty root (header end 0).
- **File creation:** `edit.rs:34-41` `FileEdit::Create { path }`; `apply.rs:23-27` materialises creations
  first and `create_tracked_file` (`apply.rs:167-176`) does `create_dir_all` + empty file + `git add -N`.
  `Overlay::record` (`overlay.rs:46-71`) seeds an empty entry for a creation and applies `Change`s in
  order, so `Create` + `Change` (insert at 1:1) is already the engine's way to author a new file
  (precedent: `backends/rust/item_move.rs:113-118`, `backends/rust.rs:144-148`).
- **Coordinate trap:** `cluster.rs` `MergedChanges` (`:209-240`) folds every `Change` of one path into one
  `FileEdit::Change` in the coordinates of the tree *as it stands*. A created file stands as `""`, so the
  skeleton text and the move's own edits to the same file (`Cargo.toml` dependency lines, `lib.rs`
  declarations) cannot be merged into one `Change` naively. Either resolve the move against a workspace whose
  overlay already holds the skeleton and emit the skeleton `Change` as a separate, earlier `FileEdit`
  (`apply_text_edits` and `Overlay::record` apply separate `Change`s in sequence), or fold the move's edits
  into the skeleton text and emit one `Create` + one full-text `Change`. The second is simpler to assert.
- **Post-apply compile gate already covers a new crate**: `runner/compile_gate.rs:201-220`
  `owning_packages` walks up from each touched file to the nearest manifest with a `[package] name`; after
  the apply the new manifest exists, so the new package is checked. The *pre*-apply baseline gate
  (`compile_gate.rs:51-80`) reads only the plan's snapshot and anchors (origin files), so a not-yet-existing
  destination is not consulted there — correct for this node (node 10 owns adding destinations to it).

### 3. The plan surface: `name` is silently ignored on crate moves today

- `plan.rs:199-202` — `name` already means *"names a module the move creates"* on `move_item`
  (docs/ft/coder/rust-code-restructuring.md:352-353; plan-schema.md:218-219), and `reparent_module` refuses it
  with "creates no module" (`plan/codec.rs:338-343`).
- `plan/codec.rs` has no rule for `name` on `move_module_to_crate` / `move_cluster_to_crate` /
  `move_test_binary_to_crate` (only `:420-433` for ops that *need* a name). So `name` on a crate move parses
  and is ignored — against the codec's own rule that a field an op cannot honour is refused
  (`:398-409` for `also`, `:411-417` for `to_file`).
- `plan/refactor_kind.rs:195-203` `moves_across_crates` = module + cluster (not the test binary).
- `.agents/skills/code-restructuring/references/plan-schema.md:126-128` — the three crate-move rows; `to` is
  "the destination crate's directory" with no word on existence.

### 4. `async-trait` is missed because the file's own import shadows the crate head

The 10-08 entry calls it "named only by an attribute macro"; the code says the cause is narrower and also
hits `use anyhow::anyhow;`:

- `crate_move/survey.rs:77-92` — for each sighting, a non-relative head is skipped when
  `is_a_built_in_root(head) || bound.contains(head)` (`:84`), **for `use` items too**.
- `bound` = `names_bound_by` (`survey.rs:125-131`) = `test_binary::names_bound_in` (`test_binary.rs:380-387`)
  + defined items + declared modules. `names_bound_in` records the name each `use` binds:
  `name_bound_by` (`test_binary.rs:458-469`) returns the **last segment** of a multi-segment path.
- So `use async_trait::async_trait;` binds `async_trait`, and its own head `async_trait` is then read as
  "bound by the file" → the sighting is dropped → `async-trait` never reaches `header.crates_named`
  (`crate_move/header.rs:94-106`) → `destination_manifest` adds nothing. `#[async_trait]` itself is a
  single-segment path and is never a sighting; it is not the cause.
- Same shape: `use anyhow::anyhow;`, `use tracing::instrument` is *not* affected (different names).
- Evidence in tree: `packages/tddy-demo-vm-service/src/demo_vm_service.rs:2` (`use async_trait::async_trait;`)
  and `:21` (`#[async_trait]`); hand fix `packages/tddy-demo-vm-service/Cargo.toml` `async-trait = "0.1"`.
- The rule that must survive: a name the file binds by `mod foo;`, by an item, or by a `use` whose head is a
  *different* name still shadows (`use Kind::*` over a local `enum Kind`, the case `survey.rs:122-124`
  documents). Only "bound solely by a `use` whose first segment is that same name, and the origin manifest
  declares it" stops shadowing.

### 5. `libc` is missed because target-specific dependency tables are never read

The 10-04 entry suspected children or nested modules; the code says it is the manifest table:

- `crate_move/manifest_edits.rs:141-150` `dependencies_of` reads only the lines under exactly
  `[dependencies]` or `[dev-dependencies]` (`Table::header`, `:90-97`). `[target.'cfg(unix)'.dependencies]`
  is invisible.
- `packages/tddy-session-lifecycle/Cargo.toml:114-115` and `packages/tddy-daemon/Cargo.toml:122-123`
  declare `libc = "0.2"` **only** under `[target.'cfg(unix)'.dependencies]`.
- Body path: `survey.rs:87-90` surveys a non-`use` path only when
  `declares_dependency_in_either_table(&manifest, head)` — false for `libc` → `libc::kill(…)` in
  `packages/tddy-cli-sessions/src/cli_session_manager.rs:240` (the engine-moved parent, not a child) was never
  surveyed → no manifest line.
- `use libc::…` shape: surveyed (a `use` head), then `moving.rs:184-196` finds no line in `[dependencies]` and
  **refuses the move** ("names `libc`, which … does not declare — there is nothing to carry across").
- The hand fix flattened the cfg: `packages/tddy-cli-sessions/Cargo.toml:11` `libc = "0.2"` under
  `[dependencies]` (now built on every target). The engine should carry the line into the same
  `[target.'<cfg>'.dependencies]` table.
- `crate_move/test_binary.rs:874-908` (`destination_dev_dependencies`) shares `manifest_edits` but has its own
  head scan; the same gap exists for `[target.…dev-dependencies]` there.

### 6. "Workspace-inherited versions" is not this workspace's convention

- Root `Cargo.toml:113-116` `[workspace.dependencies]` holds only `rstest`, `pretty_assertions`, `sysinfo`;
  across `packages/*/Cargo.toml` the only `workspace = true` lines are those three (60 / 51 / 1). No
  `[workspace.package]` table exists. Hand-built carved crates (`packages/tddy-cli-sessions/Cargo.toml`,
  `packages/tddy-demo-vm-service/Cargo.toml`) are `[package] name, version = "0.1.0", edition = "2021",
  description` plus verbatim-copied version lines — the shape `docs/dev/todo/2026-09-23-carved-crate-manifests-repeat-versions-and-tokio-feature-lists.md`
  records as debt to decide workspace-wide.
- The engine's existing rule (copy the declaring manifest's line verbatim, `moving.rs:103-110`) already carries a
  `{ workspace = true }` line as written, so a new manifest neither invents versions nor starts a second
  convention.

### 7. Test harness and fixtures to reuse

- **Library level, no server** (the bulk): `crate_move.rs` `mod tests` (`:417-…`) — `AWorkspace` with
  `.with(path, text)` on a tempdir + `Overlay`, `a_workspace_with_two_crates()` (root `members`, origin with a
  path dependency, destination), the fake `AKnownReferenceSet: ModuleReferences`, and the readers
  `applied(edit, path, workspace)`, `changed`, `renames`. A "created" helper (`FileEdit::Create` paths) is the
  one missing reader. `crate_move/cluster.rs` tests (`a_workspace_with_an_entangled_pair`, `:286`; fake at
  `:387`) for the once-per-set edits. `crate_move/survey.rs` tests (`surveying(moved)`, `:179-…`) for
  sightings — needs a variant whose origin manifest declares `async-trait` / a target table.
- **Static check parity, no server:** `tests/check_precondition_parity.rs` (`unrunnable_moves` over a tempdir,
  `a_manifest_named`, `an_origin_holding`).
- **Plan codec:** `plan.rs` tests (`:786`, `:844`, `:894` parse crate-move lines).
- **Live, rust-analyzer + `cargo check`:** `tests/move_module_to_crate_acceptance.rs` with
  `harness::a_workspace_a_module_can_move_across()` (`tests/harness/mod.rs:92-140`, root `members` list,
  `tracked_by_git`), `performing`, `assert_compiles`. Already in `.config/rust-e2e.filterset:62` and the
  `rust-analyzer` group (`.config/nextest.toml:91`).
- Observation (not this node): `tests/cluster_move_acceptance.rs` spawns rust-analyzer and says it is in the
  `rust-analyzer` group (`:14-15`), but `.config/nextest.toml:88-104` does not list it (only the filterset,
  `rust-e2e.filterset:46`).

### 8. Boundaries with neighbouring nodes (textual collisions only)

- Node 5 (`move-children`) edits `cluster.rs` / `moving.rs` / `module_files.rs` for directory children and the
  manifest of the children; this node's manifest fixes (sections 4, 5) are in `survey.rs`, `test_binary.rs`
  (`names_bound_in`) and `manifest_edits.rs`, and the creation is a new module plus `destination.rs`. A child's
  `libc`/`async-trait` is covered once node 5 reads children *through the same survey*.
- Node 7 (`move-widen`) and node 8 (`move-grouped-use`) touch `header.rs` / `crate_move/survey` consumers, not
  the head-skip rule or the manifest tables.
- Node 10 (`apply-robust`) owns adding destination packages to the *pre*-apply gate; a created crate has no
  pre-apply package to add.

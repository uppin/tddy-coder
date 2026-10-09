# Initial discovery — #reshape 10/19 `apply-robust`

Companion to [the changeset](2026-10-09-reshape-apply-robust.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — apply-robust (git-mv preflight, destination baseline, silent waits, op-tagged spawns, stale-plan message)

Read on 2026-10-09 against `4a5c42b1b` (`pleasant-tent-b7473e01`). Every claim of the claimed entries
was checked against the code; where an entry is stale it says so.

### A. `git mv` of an uncommitted file (09-09 first-cross-crate item 2) — CONFIRMED OPEN

- `apply.rs:18-39` `apply_workspace_edit` applies, in order: every `Create` (`git add -N`), every
  `Change` (text written to disk, `apply.rs:97-105`), **then** every `Rename` (`git_move`,
  `apply.rs:158-163` → `run_git(["mv", from, to])`). The order is deliberate: a test-binary move
  addresses its text edit to the path it is moving *from* (`crate_move/test_binary.rs:132-145`).
  Consequence: an untracked `from` fails at `git mv` **after** the op's text edits are on disk.
- No `ls-files` anywhere on the apply path. The only `ls-files` is `runner/comparison.rs:54`
  (`verify`'s source listing).
- What the failure leaves: `runner.rs:77-83` `commit_operation` appends an `in_flight` record with
  the pre-hashes, then `apply_workspace_edit` fails half-way. The touched files now match neither
  `pre` nor `post`, so the next open reads `ResumeDecision::Abort` (`journal.rs:323-340`) and refuses
  with `IndeterminateJournal` (`runner.rs:186-188`). A non-group op is not rolled back
  (`GroupRun::roll_back_on_failure`, `runner/group_gate.rs:166-181`, only acts on an open group).
  So the tree is half-edited **and** the plan cannot be resumed — strictly worse than the entry says.
- The git error is reported as `MalformedPlan("git mv … failed: fatal: not under version control")`
  (`apply.rs:196-208`) — the wrong class: the plan is fine, the tree is not.
- Probed in a scratch repo: `git mv` of an **intent-to-add** file (what `create_tracked_file`,
  `apply.rs:167-176`, leaves) succeeds; only a file git has never seen fails. `git ls-files
  --error-unmatch -- <path>` exits 1 for exactly that case and 0 for an i-t-a or renamed-staged file.
  So engine-created files and files an earlier op of the same run moved are not false positives.
- Rename producers (who can hit it): `crate_move.rs:157`/`crate_move/moving.rs` (module move),
  `crate_move/cluster.rs:133` (cluster), `crate_move/test_binary.rs:134` (test binary),
  `backends/rust/module_reparent.rs:80` (`reparent_module`). Node 5 adds directory children — more
  renames per op, same path through `apply_workspace_edit`.
- Both apply loops commit through `commit_operation`: the CLI's
  (`runner/entry_points/store_run.rs:342-351`) and the index daemon's
  (`packages/tddy-index-daemon/src/apply.rs:198-206`). One preflight there covers both.
- Parity points: the dry-run branch (`store_run.rs:322-335`) never calls `commit_operation`, so a
  dry run would report the op as applicable; `check --deep` resolves each op in
  `runner/rehearsal.rs:30-97` and has the resolved `WorkspaceEdit` in hand, so it can run the same
  preflight. In a dry run / rehearsal, a `from` created or renamed by an earlier op exists only in the
  `Overlay` (`overlay.rs:61`), so the preflight must treat overlay-created/renamed paths as tracked.
- Harness: `tests/harness/mod.rs:1449-1468` `a_workspace_with_a_test_binary` + `.tracked_by_git()`
  (`:327`), driven by `moving_the_test_binary_recording` (`:1495-1538`) over `fake_lsp` — a
  test-binary move asks the server nothing, so the untracked case is a no-rust-analyzer test: write
  the test binary **after** the baseline commit.

### B. Pre-apply compile gate omits the destination (09-25 test-binary entry) — CONFIRMED OPEN

- `runner/compile_gate.rs:51-80` `refuse_a_broken_baseline` checks `owning_packages` of the plan's
  snapshot keys and each `op.anchor.file()` only (`:61-65`). `op.to` (the destination crate dir,
  `plan.rs:205`) is never read; nor is `op.also` (`plan.rs:233` — origin-crate files, so harmless).
- The post-apply gate already covers the destination: `refuse_a_broken_result` (`:91-148`) checks the
  packages of every completed edit's paths (`completed_edit_paths(journal)`), which include the
  renamed-to file and the destination manifest. Hence the entry's symptom: a pre-existing destination
  break surfaced only after the apply, attributed to the plan.
- `owning_packages` (`:201-220`) pops the given path first, so passing `<to>/Cargo.toml` resolves the
  destination; a `to` with no manifest (node 9's new crate) walks up to the virtual root manifest and
  adds nothing. Better to read the destination manifest directly (`crate_move/destination.rs:27-51`
  `Destination::read` gives the declared package name) and skip a missing one explicitly.
- Kinds carrying a crate `to`: `MoveModuleToCrate`, `MoveClusterToCrate` (`plan/refactor_kind.rs:197-202`
  `moves_across_crates`) and `MoveTestBinaryToCrate` (reads `op.to`, `crate_move/test_binary.rs:50`).
  `move_item`/`reparent_module` also carry `to`, but a module path in the same crate.
- Not covered and out of this entry: third crates whose callers a crate move re-points. They are
  in the post gate, not the baseline (proposed todo).
- Harness: `tests/apply_compile_gate_acceptance.rs:86-128` already pins the baseline refusal
  wording (`cargo check --all-targets -p origin`) and "nothing written"; breaking
  `crates/destination/src/lib.rs` instead of `ORIGIN_LIB` is the new case.

### C. The apply that did not return (10-05) — root cause NOT established; two unbounded waits found

- The heartbeat landed after the incident: `backends/rust/wait.rs` is from `#sharpen` 4/8 (#591,
  2026-10-07); the incident is 2026-10-05. `wait.rs:9` states "It adds no deadline", and
  `tests/wait_heartbeat_acceptance.rs:340-370` `a_wait_has_no_deadline_however_many_beats_pass` is a
  guard for that decision. The feature doc says the same (`docs/ft/coder/rust-code-restructuring.md`
  §"A wait that lasts says so": "it adds no deadline"; and "A server should not invent a deadline its
  caller never stated").
- The wait the incident sat in is `readiness.rs:147-206` `await_answer` (it prints exactly "waiting
  for type inference at the anchor", `:153`). It ends only on: a non-null hover while not loading;
  an `unlinked-file` / `inactive-code` diagnostic; cancellation; or — **only when called with a
  bound** — a ready index leaving hover `null` for `READY_HOVER_BOUND` (30 s, `readiness.rs:26`,
  added by #542 on 2026-10-03).
- **Who passes a bound:** only the extraction inference probe (`rust.rs:1547`,
  `wait_until_resolved_within_bound`). **Unbounded:** `rename_symbol` (`rust.rs:2196` — the incident's
  op), `anchor_range` for symbol anchors (`rust.rs:2151`), the post-assist rename (`rust.rs:2295`),
  `extract_module_to_file` (`rust.rs:1960`), `references_at` (`rust.rs:1814`, the survey) and
  `signature.rs:146`.
- Two shapes keep `await_answer` waiting for ever:
  - **H0 — ready and silent at an untypable position.** Index ready (`!loading`), hover `null`, no
    diagnostic excusing it: the loop polls every `INDEXING_POLL` (2 s, `rust.rs:387`) for ever on the
    unbounded callers. This matches the incident's signature (0% CPU, last words frozen, the daemon
    answering `--status`) **and** extract-item R's (range starting on `{` or a comment, rust-analyzer +
    daemon + client at 0% CPU) — R was fixed for extractions by exactly this bound in #542
    (`selection::hover_bearing_position` + the bounded probe), but the rename path was not given it.
    For an item-anchored rename, the position is the lowered range's start
    (`rust.rs:2185-2187`, `item_anchor.rs:199-238`); a relative start on trivia (indent, attribute,
    doc comment) would hover `null`.
  - **H1 — loading and silent.** `chatter.loading()` (`chatter.rs:251-253`: reported status and not
    quiescent) stays true; the loop never applies any bound in that state (`readiness.rs:187-201`
    resets the silent clock). The last words "working: build script num-bigint run" are rust-analyzer
    re-running build scripts after the tree changed. rust-analyzer is started with no
    `cargo.targetDir` (`rust.rs:262-269` `server_settings`), so its build-script `cargo check` shares
    `target/` with the engine's own baseline `cargo check` (`compile_gate.rs:282`) — which `apply`
    runs and `check --deep` does not, the one systematic difference between the two runs in the
    incident. Lock contention would show as 0% CPU; it would not by itself explain a *permanent*
    stall once the baseline finished. Unproven.
  - H2 — a request in flight that never answers — is already bounded by the shared client's
    per-request bound (`tests/wedged_request_acceptance.rs:24-26`, ten minutes in production) and by
    cancellation; not a candidate for an indefinite wait.
- Today's heartbeat distinguishes H0 from H1 in its own line (`wait.rs:134-138`): H1 says "is still
  loading"; H0 says "has not said it is ready" — which is **wrong wording** for H0 (the server *has*
  said it is ready; it gives no hover). Worth fixing with the bound.
- Daemon impact: an apply wedged in `await_answer` holds the daemon's backend; extract-item R records
  that "every later request to that daemon queued behind it" — so an unbounded wait is a daemon-wide
  outage, not one stuck run.
- Reproduction ingredients now available that were not on 10-05: the heartbeat lines (stage,
  loading, last words, quiet-for), the daemon spawn record (#590) showing the engine's own `cargo`
  spans and whether they overlap the stall. rust-analyzer's own children (build scripts) are not in
  the record (feature doc §spawn record "What it cannot show") — `ps -o pid,ppid,stat,etime,command`
  under the rust-analyzer pid and `lsof target/debug/.cargo-lock` cover that by hand.
- Fakes: `fake_lsp` (`packages/tddy-lsp/tests/bin/fake_lsp.rs`) has `--cold-hovers N` (hover `null`
  for the first N), `--loads-crate-graph` (narrate, then report quiescent), `--never-quiescent`,
  `--hover-never-answers`, `--goes-busy-after-hovers`. `--cold-hovers <large> --loads-crate-graph`
  is H0; `--never-quiescent` is H1. Missing: a mode that keeps *narrating* while loading (to prove
  a narrating load is never ended by a silence bound) — one new flag in a file that lives in
  `tddy-lsp/tests/bin`.
- The cadence is injected (`registry_for_waiting`, `runner/entry_points.rs:125-140`;
  `RustBackend::with_wait_heartbeat`); a bound would be injected the same way so no test branches on
  being a test. `READY_HOVER_BOUND` is a `const` today, not injectable.

### D. Spawn records carry no operation (10-05 no-record item 3) — CONFIRMED OPEN

- Items 1 and 2 of the entry are delivered (`spawn_record.rs`, `spawn_record/jsonl.rs`; #590).
- The start line (`spawn_record/jsonl.rs:128-140`) carries `v, event, id, at_unix_ms, origin,
  purpose, program, argv, cwd, pid, env_names` — nothing names the plan operation.
- `ProcessStart` is `tddy-lsp`'s type (`packages/tddy-lsp/src/spawn_observer.rs:24-37`); constructed in
  two places — `tddy-lsp/src/server_body.rs:129` (the language server) and
  `spawn_record.rs:139-155`. The observer receives only a `ProcessStart`, so carrying an operation
  needs a field on it (or a second observer event).
- What runs *during* an operation, as recorded processes: the `git add -N` / `git mv` of
  `apply_workspace_edit` (inside `commit_operation`), and a group's end gate `cargo check`
  (`GroupRun::settle`, `store_run.rs:358-366`). The baseline, the result gate and the tidy are
  run-level. rust-analyzer requests are not processes. So the tag mostly lands on `git` lines and a
  group's `cargo check` — honest scope to state in the PRD.
- Progress lines name the op **index** (`store_run.rs:300-304` "op {index} of {total}: resolving …")
  and the journal records both `op` and `op_id` (`journal.rs:54-61`), so index joins already work
  for the progress stream; the stable id (survives a reorder) is not printed. The account line's
  shape is a three-renderer contract (`console::operation`; `tests/one_renderer_for_every_front_end.rs`)
  and should not change here.
- Harness: `tests/spawn_record_acceptance.rs:67-128` (`an_apply_records_every_process_…`) already
  reads a JSONL record of a test-binary move — the op-tag assertion slots in beside it.

### E. A stale plan after a failed apply says only "item changed" (leftovers item 4) — CONFIRMED OPEN

- After each committed op the plan is refreshed and flushed (`store_run/applied_op_record.rs:29-61`),
  so after a run that failed later (compile gate, `AppliedTreeDoesNotCompile`, whose message tells
  the author to `git checkout HEAD -- …`, `lib.rs:229-235`) and was rolled back by hand, the plan
  file's anchors describe the tree the run left, not the tree restored.
- The re-run is refused first by `refuse_a_stale_pending_op` (`store_run.rs:417-451`, called at
  `:83` before `open_plan_run`; the daemon calls it at `tddy-index-daemon/src/apply.rs:82`) with
  `StaleOperation { reason: "item changed" }` (`lib.rs:141-146`, `plan_store.rs:75-83`); `check`
  says the same via `stale_findings` (`store_run.rs:455-469`). A later path raises `ItemChanged`
  (`item_anchor.rs:249`, `lib.rs:105-109`). Neither consults the journal.
- What the journal knows (`journal.rs:54-94`): every `completed` record's op index/id and `pre`/`post`
  hashes, and `PlanSynced` digests — i.e. that a run of *this* plan wrote it back after op N. If every
  completed record's touched files now hash to their `pre`, the run's edits are provably undone.
  The journal stays at `.restructure/<plan>-<digest>/` (`runner.rs:267-292`) unless removed; if the
  author removed it, nothing can be said and the message stays as it is.
- The skill already documents the behaviour (`.agents/skills/code-restructuring/SKILL.md:104`);
  the refusal does not.

### F. Coordination notes for neighbouring nodes

- **Node 4 (extract-method-clean), item R:** the *hang* half of R is already a bounded refusal for
  extractions since #542 (`rust.rs:1547`, `readiness.rs:136-145,187-198`): a range on `{` or a
  comment now refuses after 30 s with "Start the range on an expression it can type". Node 4 owns
  making such a range *succeed*; this node owns bounding every *other* `await_answer` caller.
- **Node 5 (move-children):** more `Rename`s per op go through the same preflight — no extra work,
  but its fixtures must commit the children they move.
- **Node 9 (new-crate):** a destination that does not exist yet has no baseline; this node skips a
  missing destination manifest explicitly so node 9 does not have to touch the gate.
- **Node 7 (move-widen)** claims the 09-09 file; this node fixes item 2 and node 7's wrap narrows it.
  **Node 3 (tidy-facades)** claims the leftovers file; this node fixes item 4.

### G. Affected packages

- `tddy-code-restructuring` — all five changes.
- `tddy-lsp` — `ProcessStart` gains the operation context (`spawn_observer.rs`, `server_body.rs:129`,
  `tests/spawn_observer_test.rs`); `tests/bin/fake_lsp.rs` gains a narrating-load mode.
- `tddy-index-daemon` — `status.rs:21-80` `status_of` is an exhaustive match, so each new
  `RestructureError` variant needs a class; its apply loop needs nothing else (it commits through
  `commit_operation`).

# Initial discovery — #reshape 12/19 `anchors-outline`

Companion to [the changeset](2026-10-09-reshape-anchors-outline.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — anchors-outline (the `anchors` command at repo scale, static item-anchor checks, `FileHint.modified`)

Paths are relative to `packages/tddy-code-restructuring/src/` unless noted. Measured against the working
tree at `4a5c42b1b` (2026-10-09). No engine commit has touched `src/` since `480c84481` (#595).

### 1. `broken-restructure-anchors-empty-outline.md`: the record's diagnosis points at dead code

**The `anchors` command no longer reaches `places_of`.** The record quotes the refusal
"`GoalId` is not an item `…/ids.rs` defines at module level" and diagnoses `places_of`
(`backends/rust.rs:2335` then, `:2380` now). That text exists in one place only, `places_of`
(`backends/rust.rs:2389`). Its only caller is `anchor_opening` (`:1410-1446`), whose only caller is
`LanguageBackend::anchor_for` (`backends/rust.rs:1198-1205`, trait default `registry.rs:91-107`).
**No production code calls `anchor_for`.** `grep -rn anchor_for` over `tddy-code-restructuring`,
`tddy-index-daemon` and `tddy-tools` finds the two definitions and three test call sites only:
`tests/cancellation_acceptance.rs:85`, `tests/workspace_root_acceptance.rs:185`,
`tests/spawn_record_acceptance.rs:402`. #537 (`ddd599028`, 2026-10-02) removed the runner's
`.anchor_for(` call and put `items_anchor` in its place (`git show ddd599028`, runner hunk line 302).

**The live path today** is `runner::item_anchors` (`runner/entry_points/anchor_entry_points.rs:40-75`),
then `item_anchor::items_anchor` (`item_anchor.rs:343-366`), then `ItemResolver for BackendRegistry`
(`registry.rs:168-184`), then `RustBackend::resolve_item` (`backends/rust/item_path.rs:335`), then
`outline_of` (`:258-276`: `start`, `did_open`, `settled_outline`), then `walk` (`:107-131`). That path
words an absent name differently: "`<item>` is not declared in <file>: nothing there is named
`<segment>`" (`item_path.rs:41-43`). The `--at` form goes through `item_enclosing_opening`
(`item_path.rs:288-325`) over the same `outline_of`.

Dead with `anchor_for`, and reached by nothing else (grep): `anchor_opening` (`rust.rs:1409-1446`),
`module_outline` (`:1727-1739`), `OutlineItem` (`:2356-2378`), `places_of` (`:2380-2399`),
`refuse_non_adjacent` (`:2401-2418`) and the trait default (`registry.rs:91-107`). That is about 145
production lines, about 125 of them in `backends/rust.rs`, which `#reshape` 17 has to bring to ≤500.
`attached_trivia_starts_at` and `outline_is_empty` stay: `item_path.rs:10` and `rust.rs:1724, 2244`
use them.

**The warm path works at repo scale, as measured from the index daemons' own logs.** The logs under
`$TMPDIR/tddy-index-*.log` are from daemons that `./run-index-daemon` started for tddy-coder worktrees
on 2026-10-07 and 2026-10-08. They hold **103 `anchors` requests answered and 3 refused**, on
workspaces that were already warm (`tddy-index-2405274265.history.log` 60/1, `.log` 4/0;
`tddy-index-3139073760.history.log` 4/0, `.log` 10/1; `tddy-index-3154050357.log` 25/1). Each
answer took 44–448 ms, and one took 2m54s. All three refusals were the request's own mistakes, none
of them an empty outline:
- `` `…svc_demo_vm_ports::<DemoVmService for DemoVmServiceImpl>` is not declared … nothing there is named `<DemoVmService for DemoVmServiceImpl>` `` — `impl Trait for Type` was spelt where the path syntax wants `<Type as Trait>`. The refusal does not say so (see proposed todo T1).
- twice, `` `DaemonSessionHost::announce_worktree_ready` is not a bare item name of module … `` — an impl member passed to `--items`, which takes module items only (`item_anchor.rs:370-385`).

**Hypothesis checked and refuted:** "a fresh bridged backend on a warm daemon never sees `quiescent`".
Every request builds a new `RustBackend::from_lsp_client` with `chatter: ServerChatter::default()` and
`indexed: false` (`rust.rs:606-635`). But `LspClientBridge::notifications_to_fold` appends the client's
**last** `experimental/serverStatus` to every drain (`backends/lsp_bridge.rs:104-107`;
`tddy-lsp/src/client/notifications.rs:80-90` keeps it "whoever drained it"). `request_settled` folds
that drain into the chatter after each bridged request (`rust.rs:844-848`). On a warm server, then,
`outline_is_the_servers_answer` (`rust.rs:1723-1725`) learns quiescence from the first request.

**The cold path's `lsp server exited` hides why the server never started.** The cold CLI builds an
`LspRegistry` around `LaunchSpec::new("rust-analyzer")`, a bare program name looked up on `PATH`
(`restructure_cli.rs:257-266`). It calls `get_or_spawn` after printing "acquiring shared
rust-analyzer client" (`restructure_cli.rs:70-74`). Inside `tddy-lsp`, the server task returns
`TaskStatus::Failed { message }` in three cases:
- the program could not be spawned: "failed to spawn language server 'rust-analyzer': …" (`server_body.rs:114-123`);
- `initialize` failed (`server_body.rs:213-232`);
- the output channel is missing (`:145-153`).

In every case the task drops `client_tx`, and `spawn_service` turns the dropped one-shot into a bare
`LspError::ServerExited` (`tddy-lsp/src/registry.rs:195-199`). **The message is thrown away.**
`restructure_cli.rs:74` adds only the context "rust-analyzer LSP". The output is therefore exactly the
record's `Error: rust-analyzer LSP / Caused by: lsp server exited`, whatever the cause.
`rust-analyzer` is not on `PATH` outside the nix dev shell on this machine (`which rust-analyzer`:
not found), and in that case the cold path prints exactly this. The harness says the same thing:
`tests/harness/mod.rs:556` reads "rust-analyzer starts — the nix dev shell puts it on PATH". **Not
reproduced:** which cause hit `#carve` 5/11. The record does not say whether that run was inside the
dev shell. `tddy-lsp/tests/spawn_observer_test.rs:151-170`
(`a_server_that_exits_before_the_handshake_is_reported_ended_with_its_exit_code`) pins
`Some(LspError::ServerExited)` for a fake server started with `--exit-immediately`. A fix changes that
assertion. `tddy-index-daemon/src/status.rs:141` maps `ServerExited` to `Status::unavailable` and
would need a mapping for any new variant.

**The cold run leaves no spawn record for `anchors`.** `ColdRunSpawnRecord` defers every line until
`.restructure/` exists (`spawn_record/deferred.rs:23-58`). `anchors` never creates that directory, so
a failed cold `anchors` leaves no trace of the spawn (see proposed todo T3).

**Self-spawned and registry-spawned servers launch differently.** `RustBackend::start` pins the
toolchain itself (`RUSTUP_TOOLCHAIN`, `CARGO`/`RUSTC` set to the real binaries; `rust.rs:752-793`, with
the comment on the 600 s rustup-proxy stall). The registry path every front end uses runs the bare
`rust-analyzer` from `PATH` with no such pinning. This is out of this node's scope (see proposed todo T2).

**Already in place from #537:** `settled_outline` (`rust.rs:1697-1718`) believes an empty outline only
when the index is loaded or the server has been seen quiescent, and refuses a degraded index. A
live-rust-analyzer test covers that: `tests/anchors_command_acceptance.rs`, registered in
`.config/rust-e2e.filterset`. It notes it is "also the empty-outline defect's regression test"
(`:1-4`). **Still open:** a server that never sends `experimental/serverStatus` still waits on an
empty outline until its caller cancels. `docs/item-anchors.md` § The outline wait says so, and this
node does not change it.

### 2. `2026-10-02-static-check-cannot-verify-item-anchors.md`: half of it is already done

- The todo names `runner/entry_points.rs`. The function now lives at
  `runner/entry_points/check_entry_points.rs:364-381` (`unresolvable_without_a_server`), called from
  `check_plan` at `:233-239`. It emits one finding per item-anchored operation: "`{op}` in `{file}`
  anchors by item, which only a deep check can resolve, so this static check did not examine it —
  run `check --deep`". It does not look inside the anchor.
- **"The relative range against the item path's shape" is already checked when the plan is parsed.**
  `Anchor::validate` (`plan.rs:65-110`) refuses a range given at one end only, a position not counted
  from 1, a range that ends before it starts, an empty `items`, and an `items`/`fingerprints` count
  mismatch. Only the length check ("reaches outside the item", `item_anchor.rs:102-148`) needs the item,
  and so a server.
- **What is left needs no server: the crate and module prefix.**
  - `module_path_of(root, file)` (`item_anchor.rs:46-77`) reads only `Cargo.toml` through
    `owning_package` (`item_anchor/package_lookup.rs`). It refuses a file that is not under `src/`, or not
    in any package.
  - The prefix comparison is `segments_below` (`backends/rust/item_path.rs:347-370`), private to the
    Rust backend, though nothing in it is Rust-specific. It refuses "`{item}` is not in {file}, which is
    module `{m}`" and "`{item}` names the module {file} is, not an item in it".
  - `items_anchor` (`item_anchor.rs:343-366`) applies the module rule to `--items` names through
    `qualified_in_module` (`:370-385`).
- Moving `segments_below` into `item_anchor.rs` as a shared rule lets the static check and the deep
  resolver refuse in **the same words**, which is the check/apply parity the static pass exists for.
  The deep-path words are pinned by `tests/item_anchor_acceptance.rs:200-216`
  (`a_module_prefix_that_does_not_match_the_file_is_refused`).
- Existing pins on the static finding's wording, which any rewording must keep:
  `tests/item_anchor_acceptance.rs:454-469` (checks for `anchors by item` and `check --deep`),
  `tests/repoint_call_plan_acceptance.rs:395` and `tests/retarget_impl_plan_lines.rs:295` (both check
  for "anchors by item, which only a deep check can resolve").
- Harness: `checking_the_plan(fixture, plan, deep: false)` (`tests/harness/mod.rs:2494-2530`) runs
  `runner::check` with `client: None` and starts **no server**. A static-only test binary can
  therefore use `a_workspace_holding_files` (`:2643`) and `a_hinted_plan_of`, and stay **out** of
  `.config/rust-e2e.filterset` and the `rust-analyzer` nextest group.

### 3. `dead-code-plan-filehint-modified.md`: confirmed, and removing it is safe

- Field: `plan.rs:126-134` (`FileHint { sha256, modified: Option<String> }`, with the doc comment
  "Never read; it is there for a person"). It is re-exported publicly at `lib.rs:37`.
- Writer: `hint_of` (`plan/codec/file_hint.rs:7-15`), which pays an `fstat` per file. `rfc3339`
  (`:20-47`) exists only to format that value. Its one other user is `plan.rs`'s test module, through
  the `#[cfg(test)]` re-export `plan/codec.rs:224-227`, which carries a `TODO(sharpen)` saying so.
  `file_hint.rs:1` carries a second `TODO(sharpen)`.
- Readers outside tests: **none**. `grep` across `packages/` (Rust, TS, proto) finds the definition,
  the writer, `plan.rs` unit tests (`:1257, 1275, 1337, 1370, 1387, 1394`) and the harness fixture
  `tests/harness/mod.rs:255`. Every other hit is unrelated (`content modified`, `tddy-vm`,
  `tddy-index-daemon/src/tree_changes.rs:40`'s own `SystemTime`). The drift report reads
  `sha256` only (`runner.rs:195-205`, `Plan::drifted_hints`).
- **Old plans keep parsing after removal.** `HintedHeader` (`plan/codec.rs:40-44`) and `FileHint`
  carry no `deny_unknown_fields`. Only `RefactorOp` does (`plan.rs:192`). A v2 header that still
  carries `"modified"` is read with the key ignored, and the next rewrite drops it.
- A reader instead of removal: the drift report could say when the file changed. But the stored value
  is the time **at snapshot**, and the drift report wants the time *now*, which needs no stored value.
  Removal is therefore the answer that fits the code.
- Docs that describe the field:
  - this node edits `.agents/skills/code-restructuring/references/plan-schema.md:14,17`;
  - the wrap edits, through the changeset, `docs/item-anchors.md:91,99` and `docs/plan-store.md:73`;
  - the wrap edits `docs/ft/coder/rust-code-restructuring.md:91,163`.

### 4. Reference only: `2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md`

The item-anchor half was closed by #539 (`plan_store/refresh.rs`). The v1 `range` rebase
(`snapshot --rebase <ref>`) stays open and is deferred, as the brief says. Nothing in this node touches
`refresh.rs`.

### 5. Test harness and fixtures this node would use

- `tddy-lsp/tests/spawn_observer_test.rs`: `a_registry_over_the_fake(&["--exit-immediately"], …)`
  (`:151-170`) already starts a server that exits before the handshake. A missing program can be
  written as a `LaunchSpec` naming a path that does not exist. Both are library level, with no
  rust-analyzer.
- `tddy-code-restructuring` unit tests in `plan.rs` (`a_v2_header_carries_its_file_hints`,
  `:1253-1280`) for the header round trip.
- A new static-only binary, `tests/static_check_item_anchors.rs`, built on `checking_the_plan(…, false)`
  and `a_workspace_holding_files`.
- Repo-scale measurement: a built `tddy-tools`, `./run-index-daemon` for the warm leg, and
  `TDDY_INDEX_SOCKET` unset for the cold leg, both inside `./dev` and outside it. This is a recorded
  manual measurement, not a CI test: a cold index of this workspace takes 6–18 min
  (`CLAUDE.md` § `./run-index-daemon`; the record's own 18-minute first call).

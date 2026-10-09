# Initial discovery — #reshape 14/19 `tests-follow`

Companion to [the changeset](2026-10-09-reshape-tests-follow.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — tests-follow (sibling `#[cfg(test)] mod x_tests;` modules of moved code)

Paths relative to `packages/tddy-code-restructuring/src/` unless noted. Read against `4a5c42b1b` (2026-10-09).
Node 5 (`feature/reshape/move-children`) is read from its approved PRD and discovery notes; its changeset is not
written yet.

### Conclusions

- **The claimed todo describes two different shapes, and node 5 already owns one of them.** In the R9 run
  (`#carve` 21/21, commits `e4e1f28eb` + `95bf932d0`) four test files were hand-moved:
  - `claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs` is **declared inside the moved file**
    (`claude_cli_spawn_steps.rs`, today `packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs:317-318`).
    It is a directory child. Node 5's carried-file set (`module_files::files_of`) takes it, because
    `items_of_module` (`crate_move/source_scan/module_items.rs:53-71`) reads a `mod` after an attribute
    like any other `mod`. **Not this node's.**
  - `conversation_spawn_wiring_tests.rs`, `host_session_socket_tests.rs` and
    `session_acting_identity_tests.rs` are **siblings**: declared `#[cfg(test)] mod …;` in the *parent*
    `connection_service.rs` (`git show 4a5c42b1b^:packages/tddy-session-lifecycle/src/connection_service.rs`,
    lines 506, 509, 540), which stays in the origin. Nothing in node 5 reads them. **This node.**
- **A test file reached through a `#[cfg(test)] mod x;` declaration is not read as test code.** The survey's
  `in_test` (`crate_move/survey.rs:39-41`, set at `:113`) comes from `sighting_walk`
  (`crate_move/source_scan/sighting_walk.rs:44, 53-58, 76, 99, 115`), which only sees attributes *inside the
  text it scans*. An out-of-line test file has its `#[cfg(test)]` in the parent, so every path in it reads
  as production code. Consequences for any test file a move carries, node 5's children included:
  its crates go to `[dependencies]` instead of `[dev-dependencies]` (`header.rs:101-105`; R9's hand-added
  `[dev-dependencies] tempfile = "3"`), and a path back into the origin is an edge that
  `refusals::refuse_a_dependency_cycle` refuses, where under `#[cfg(test)]` it would not be
  (`header.rs:56-57`). Node 5's PRD says only "`[dev-dependencies]` when only its `#[cfg(test)]` code names
  it" (in-file); it does not inherit the declaration's gate. This node needs it for siblings, so it is
  proposed here for both (F1).
- **The header pass and the body precondition disagree about co-moving re-exports.** `reach`
  (`crate_move/header.rs:139-171`) asks `travels_with` of `path.resolved` only (`:142-144`).
  `stays_behind_through_a_body` asks it of `path.defined_at` (`crate_move/preconditions.rs:103-106` →
  `module_left_behind` `:126-137`). R9's `use super::recipe_enables_conversation_spawn;` resolves to
  `origin::connection_service::recipe_enables_conversation_spawn`, which is not co-moving, and is
  *defined at* `origin::connection_service::conversation_spawn::…` through
  `pub(crate) use conversation_spawn::*;` (old `connection_service.rs:320-321`; `reexports::followed`
  `crate_move/reexports.rs:35-52`, glob walk `:128-135`). So the header pass re-points it at the origin
  (`tddy_session_lifecycle::connection_service::conversation_spawn::…`, an edge back) instead of
  `crate::conversation_spawn::…`, which was R9's hand fix. A member file can hit this too (a member reaching
  a co-moving sibling through its parent's glob); a sibling test module hits it routinely, because the
  parent's globs are how siblings see each other.
- **The other two R9 hand fixes are already produced by the existing rules once the file is surveyed.**
  `crate::user_sessions_path::username_for_uid` and `crate::connection_service::AttachmentProgressSink` are
  facades in the origin; `reexports::followed` takes them to `tddy_session_activity::…` and
  `tddy_session_files::attachment_progress::…`, and `reach`'s last arm writes `defined_at`
  (`header.rs:166-170`). Nothing new is needed for them.
- **R9's one staying test module is the "mixed" case, and staying was right.** `stack_child_spawn_tests.rs`
  names `use super::*;` (the staying parent) and builds a `DaemonSessionHost` (lifecycle). A rule "every
  origin path it names is in the moving set" keeps it in the origin with no special case.

### 1. Where a sibling test module is visible today

| Piece | Where | What it does today |
|---|---|---|
| member's declaring file | `ModuleHome.declared_in` (`crate_move/module_home.rs:36-46`, set `:80-85`) | The crate root for a top-level member, the parent's own file for a nested one. This is the file whose other `#[cfg(test)] mod` lines are the siblings. |
| one rename per member | `crate_move/cluster.rs:133-136` | Only `member.source` is renamed (node 5 widens this to the carried tree). |
| travelling set | `crate_move/cluster.rs:118` | `members.iter().map(|m| m.source)`. `surveyed` drops references whose file is in it (`crate_move.rs:221-229`). A sibling test file is **not** in it, so with `reexport: "none"` its references to moved items come back as caller rewrites (`crate_move.rs:232-251`) — which would collide with a header re-point if the file then also moved. Following test files must join `travelling` before the survey runs. |
| declaration edits in the origin | `facade_writer::left_behind` / `leaving` (`crate_move/moving/facade_writer.rs:31-104`) | Rewrites only members' `mod` lines (`declaration_of` `:136-150` via `manifest_edits::module_declaration`). The span is the `mod` line alone, no attribute, no doc comment. |
| declaration edits in the destination | `declared_in_destination` (`facade_writer.rs:291-313`) | `pub mod <module>;` in sorted position (`manifest_edits::insert_module_declaration_sorted` `:28`). A test module needs `#[cfg(test)]\nmod <name>;`, private, which this does not write. |
| dev-dependencies | `Move::destination_manifest` (`crate_move/moving.rs:105-150`) | Takes `dev_crates_named`; copies lines from either origin table (`manifest_edits.rs:123-139`). Ready to take a test file's crates once they are marked test. |
| static check | `runner/entry_points/check_entry_points.rs:254-262` | `crate_move::unrunnable` + `stranded_siblings`. Neither reads a sibling test module. |
| deep notes | `Resolution.notes` (`edit.rs:71-75`); `MoveClusterToCrate` returns `Resolution::of(…)` with none (`backends/rust.rs:1322-1331`) | Node 5 adds its "`N` file(s) move with …" note here; this node adds one per following / staying test module. |

`move_module_to_crate` resolves through the same `resolve_cluster` (`travelling_alone`, `cluster.rs:61-67`),
so one change covers both operations.

### 2. Reading a test declaration

- `ChildModule` (`module_items.rs:12-20`) has `name`, `body`, `is_public`. No attribute is recorded.
- The `cfg(test)` recogniser exists: `Scan::cfg_test_attribute` (`crate_move/source_scan.rs:85-87`, private to
  `source_scan`; `module_items` is a child module, so it can call it). It accepts `#[cfg(test)]` and
  `#[cfg(all(test, …))]` only (`source_scan.rs:7-12`), the same reading every other `in_test` decision takes.
- `#[path = "…"]` on a test declaration: zero occurrences in `packages/*/src` (grep). Node 5 refuses a
  carried `#[path]` child; for a *sibling* the honest answer is "not followed, stays, note" (it is not part
  of what the move must carry).
- Doc comments above the attribute are common: `/// PRD: …` then `#[cfg(test)]` then `mod …;`
  (`packages/tddy-session-lifecycle/src/connection_service.rs:283-285, 444-446, 460-462`).

### 3. How common the shape is

- 46 out-of-line `#[cfg(test)] mod x;` declarations in `packages/*/src` (grep `-A1 '#\[cfg(test)\]'`).
  The lifecycle family (`tddy-session-lifecycle/src/connection_service.rs`, ~30) and `tddy-host-service/src/lib.rs`
  (6) declare them as siblings of the code they test; `tddy-agent-launch/src/lib.rs:49-54` holds R9's three.
- `tddy-code-restructuring` itself (the second stack's target) has one: `runner/tidy.rs:37-38`
  (`mod wide_facade_tests;`), a **child** of `tidy`, so node 5 carries it. Its own inline `mod tests { … }`
  blocks move with their file. So this node matters for the daemon-side crates more than for stack 2.

### 4. Classification sketch (to be fixed in the PRD)

For each `#[cfg(test)] mod t;` declared in a member's `declared_in` file, when that file itself stays:

1. read `t`'s files (node 5's "files below and where each lands" helper, `module_files.rs`);
2. survey each with `survey_moved_file` (`survey.rs:67`) at module path `declared_in`'s path + `t`;
3. a path *names the origin* when `defining_crate == origin`; it *reaches the moving set* when
   `travels_with` holds for `resolved` **or** `defined_at` against the co-moving set (members, node 5's
   carried modules, and modules earlier operations already moved are no longer the origin's after the overlay);
4. `t` follows when every origin-naming path reaches the moving set and at least one does. It stays otherwise.

Multi-operation plans fall out of the overlay: a test of `a` and `b` moved by two operations stays at the
first (it names `b`, still in the origin) and follows at the second (`a` already resolves to the destination
through its facade or re-pointed path; `reexports::followed` reports the destination as `defining_crate`).

### 5. Not this node's (found while reading)

- **Same-crate operations** (`reparent_module`, `extract_module`, `move_item`) do not take sibling test modules
  either; `reparent_module` carries children only (`backends/rust/module_reparent/survey.rs:196-240`).
  Unclaimed → proposed todo.
- **Inline `#[cfg(test)] mod tests { … }` in the parent** that tests a moved sibling cannot travel as a file;
  splitting it is an `extract_module` job. Unclaimed → proposed todo.
- **Test modules declared anywhere other than the member's declaring file** (a crate-root `mod foo_tests;`
  testing a nested `a::foo`). Unclaimed → same proposed todo as above, as a widening.
- **Budget counter stopping at an out-of-line `#[cfg(test)] mod x_tests;`** is node 15's (developer reassignment).

### Test harness and fixtures to use

- Library level, no server: `resolve_cluster` / `MovingCluster` (`crate_move.rs:298-299`) with a fake
  `ModuleReferences` (`cluster.rs:343-392` is the pattern). Node 5 plans `tests/crate_move_children.rs` with its
  own fake; this node adds `tests/crate_move_test_modules.rs` and reuses that fake if node 5 places it in
  `tests/harness/` (F6).
- Static parity: `tests/check_precondition_parity.rs` with `harness::a_workspace_holding_files`
  (`tests/harness/mod.rs:2643`) and `checking_the_plan` (`:2494`).
- `items_of_module` unit tests for the new test flag go in `source_scan.rs`'s test module (where
  `a_path_under_cfg_test_is_marked_and_the_one_after_it_is_not` sits, `source_scan.rs:387`).
- Live, compiled: one case in `tests/cluster_move_acceptance.rs` (already in `.config/rust-e2e.filterset` and the
  `rust-analyzer` nextest group, so no registration).

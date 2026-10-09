# Initial discovery — #reshape 2/19 `multi-seam-extract`

Companion to [the changeset](2026-10-09-reshape-multi-seam-extract.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — multi-seam-extract (what the server sees while a plan cuts several seams)

Paths relative to `packages/tddy-code-restructuring/` unless noted. Read against the tree at
`4a5c42b1b` (2026-10-09).

### Verdict on the claimed entries

| Entry | Claim in the record | State today |
|---|---|---|
| `2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md` row 1 — "reference survey rewrote a reference as `<mod>::<Item>`" | open | **Fixed for the moved body**, open for references *in* a sibling file (see R1) |
| same, row 2 — import pass leaves helpers unqualified in a later seam | open | **Open** (R2) |
| same, row 3 — visibility restored to private while a sibling seam still reaches the item | open | **Open** (R3) |
| same — "`check --deep` held per op and let the plan through" | open | **Open, and worse than recorded**: a deep check never shows the server the files earlier ops create (R4) |
| `2026-09-24-restructure-apply-gaps-…` J — `use super::strip_resize;` unresolved after nine seams | cause "not investigated", suspected cross-op narrowing | **Open; same root cause as R3** (R5) |

### The root cause is the server's view, not the three passes

The 09-18 record says "`extract_module` reads 'outside the new module' as 'the parent file'". The
passes do not read the parent file for reach; they ask rust-analyzer, and the server is never told
about the files earlier operations of the same plan wrote.

- **Reach is already counted across files.** `reach_of` (`src/backends/rust.rs:1833`) marks any
  reference whose uri is not the anchor's as `stranded_in` + `from_outside` (`rust.rs:1849-1856`).
  `restore_visibility` keeps a widening whenever `item.reached_from_outside`
  (`src/backends/rust/visibility.rs:82`). So a reference from a sibling-seam file *would* keep the
  item widened — if the server returned it.
- **Only the anchor's document is opened.** `resolve_opening` does `self.did_open(&uri, &original)`
  for the anchor alone (`rust.rs:1364`, `rust.rs:1376`); `survey_moved_items` (`rust.rs:1651`),
  `survey_impl_members`, `reach_of`, `restore_imports` (`src/backends/rust/imports.rs:40`) and
  `unresolved_names` all query that one uri. Every document is closed at the end of each entry point
  (`src/backends/rust/documents.rs:39-58`, wrapped at `rust.rs:1208`), handing files "back to disk".
- **`check --deep` and `apply --dry-run` keep earlier ops in memory only.** `Rehearsal::rehearse`
  records each resolved edit into an `Overlay` (`src/runner/rehearsal.rs:62-66`); `Overlay::read`
  answers the engine's own reads (`src/overlay.rs:34-39`) but nothing sends the overlay's files to
  the server. A file created by op 1 (`refusals.rs`) exists only in the overlay, so for op 2 the
  server sees the parent's projected text (`mod refusals;`) pointing at a file that does not exist.
  The same holds for `apply --dry-run` (`src/runner/entry_points/store_run.rs:322-333`) and the
  daemon's loop (`packages/tddy-index-daemon/src/apply.rs:190-194`).
- **A real `apply` relies on rust-analyzer's own file watcher.** The client advertises no
  `workspace.didChangeWatchedFiles` capability (`client_capabilities`, `rust.rs:196-236`) and nothing
  in the crate sends `workspace/didChangeWatchedFiles` (no match in `src/`). The index daemon
  documents that this watcher is **blind on this repository's workspace** and tells the server of
  disk changes only *between* requests (`packages/tddy-index-daemon/src/tree_changes.rs:1-22`,
  `packages/tddy-index-daemon/src/index.rs:276-300`). Within one apply request, a file written by op
  `k` is unknown to the server when op `k+1` is resolved. On a small fixture the watcher does see it,
  which is why `tests/sibling_module_paths_acceptance.rs` passes.

So one mechanism produces all three 09-18 rows and gap J: **every question about op `k+1` is asked
of a tree that lacks the files ops `1..k` wrote.**

### Row by row

- **R1 — qualified paths.** The in-body half is fixed: `inline_paths::rerooted_module`
  (`src/backends/rust/inline_paths.rs:23-36`, header 9-14) writes `super::<sibling>::X` for a path
  whose first segment is a module the parent declares, which covers `destination::Destination`
  carried into `refusals.rs`; pinned by `tests/sibling_module_paths_acceptance.rs` (two seams, apply,
  compiles) and `tests/inline_paths_acceptance.rs`. Still open: a reference **in an earlier seam's
  file** to an item *this* seam moves. The assist's edit is kept for the anchor's uri only
  (`extract`, `rust.rs:2266-2271`), so nothing rewrites it; with `reexport: none`
  `seam_survey::refuse_stranded` (`src/backends/rust/seam_survey.rs:124-141`, called at
  `rust.rs:1559`) is the guard — and it cannot fire, because the server never reports the reference.
- **R2 — imports.** `restore_imports` reads the names unresolved *before* the cut off the parent's
  text (`imports.rs:57-61`) and weighs only names "the seam lost". In a rehearsal the parent's
  projected text already has `helper` unresolved (its glob facade `pub use helpers::*` names a module
  the server cannot find), so `helper` is filtered out as not-lost and the new module gets no import.
  A name with no offer is "skipped rather than refused" (`imports.rs:86-88`), so nothing is reported.
- **R3 — visibility.** With the sibling file invisible, `reach_of` finds no reference outside the
  range, `reached_from_outside` is false, and `restore_visibility` puts the item back to private
  (`visibility.rs:82-98`, `relative_visibility::put_back`). The widening it declined is reported
  nowhere, exactly as the record says.
- **R4 — `check --deep` parity.** Beyond R1–R3, a deep check prints an operation's `notes`
  (`src/runner/entry_points/check_entry_points.rs:301-303`) but **not its widenings**: `Rehearsed`
  carries `survey`, `refusal`, `notes` (`src/runner/rehearsal.rs:19-27`) and drops
  `Resolution::report`. `apply` and `apply --dry-run` print them through `report_visibility`
  (`src/runner/entry_points.rs:218-227`, called at `store_run.rs:316`). So even a correct rehearsal
  cannot show the widening a sibling forces. `check --deep` also never compiles — stated in
  `docs/ft/coder/rust-code-restructuring.md:900-902`.
- **R5 — gap J.** Seam A (`pty_handle`) moves `PtyHandle::send_input`, whose call to `strip_resize`
  becomes `use super::strip_resize;` in `pty_handle.rs`. Seam B moves `strip_resize` into its own
  module behind a glob facade. With `pty_handle.rs` invisible to the server, B narrows
  `strip_resize` to private (R3), the glob re-exports nothing private, and A's `use` dangles
  (`E0432`). The record's suspicion was right about the shape; the cause is the server's view.

### What exists to reuse

- **Per-plan state already lives on the backend.** `RustBackend::claimed` (`rust.rs:473-481`) carries
  module names earlier operations introduced across one run, used by `check` (`rust.rs:1168-1185`).
  The same lifetime fits a record of the files earlier operations wrote: the registry builds one
  backend per run (`runner::registry_for_waiting`, used at `tddy-index-daemon/src/apply.rs:122`), so
  the record dies with the run and needs no change to `Workspace` (`src/registry.rs:25-35`, ~25
  struct-literal sites across this crate's tests and `tddy-index-daemon`).
- **`Workspace::read` already answers with the projected text** in all three modes — overlay in a
  rehearsal/dry run, disk in a real apply — so opening a written file with `workspace.read(path)`
  gives the server exactly the tree the plan has produced so far, without the backend knowing which
  mode it is in.
- **`did_open` / `closing_what_it_opens`** (`documents.rs:25-58`) already track and close every
  document an entry point opens; projected files opened through it are closed at the end of the
  operation like any other.
- **Edits name their files.** `FileEdit::{Create, Change, Rename}` (`src/edit.rs`) — the record is
  "paths created or changed, renamed paths followed from → to", folded the way
  `Overlay::record` folds them (`overlay.rs:46-71`).

### Harness and fixtures

- `tests/harness/mod.rs`: `a_crate_whose_second_seam_calls_what_the_first_moved` (`:2250`),
  `an_extract_of_outer_functions_into_a_file` (`:2290`, item-anchored, `to_file: true`),
  `applying_a_plan_of` (`:2452`), `checking_the_plan` with `deep: true` (`:2493-2530`),
  `assert_compiles` / `assert_compiles_with_its_tests` (`:908-920`), and
  `checking_the_plan_with` (`:2504`), whose `adjust` hook can capture `options.account` — where
  widenings are printed. The only dry-run helper that keeps lines (`:1495-1538`) is specific to the
  test-binary move and a fake server, and captures `progress`, not `account`. New fixtures follow the `OUTER_MODULE`
  (`crates/origin/src/outer.rs`) pattern.
- **A deterministic red needs the overlay, not the watcher.** On a small fixture rust-analyzer's own
  watcher sees files a real `apply` wrote, so an apply-and-compile test can pass on master. A
  rehearsal (`check --deep`, `apply --dry-run`) never puts the earlier files on disk, so it is blind on
  every machine: those are the tests that are red on master. A harness helper that resolves a plan's
  operations one after another through one backend and one `Overlay` (both public: `Overlay`,
  `Workspace`, `RustBackend::from_lsp_client`, as `resolving_after_a_check_of` does at `:455-495`)
  exposes each operation's edit for assertion.
- Live binaries must be listed in `.config/rust-e2e.filterset` and the `rust-analyzer` group in
  `.config/nextest.toml` (`:88-105`). Drift found: `sibling_module_paths_acceptance` and
  `inline_paths_acceptance` are in the filterset (`:57`, `:73`) but **not** in the nextest group.

### Not this node's, noticed on the way

- 09-24 **X, second half** — a private item widened to `pub(crate)` where `pub(super)` was the reach
  needed, tripping `private_interfaces` (`visibility.rs:15` `WIDENED`). Not claimed by any `#reshape`
  node.
- 09-24 **G** (attribute-macro imports) — no test or code mentions `async_trait` or attribute paths in
  the import pass; "likely fixed" in the whole-work discovery is unverified. Unclaimed.
- 09-24 **L** (`()` types for an `extract_method` after an `extract_module`) — a rehearsal resolves the
  `extract_method` against a server that has not seen the module file, so this node's projection may
  bear on it; not claimed, worth re-measuring after this node.

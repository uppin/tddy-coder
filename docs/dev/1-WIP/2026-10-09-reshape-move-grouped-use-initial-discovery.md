# Initial discovery — #reshape 8/19 `move-grouped-use`

Companion to [the changeset](2026-10-09-reshape-move-grouped-use.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — move-grouped-use (grouped `use` lines, glob facades and `pub(in …)` in cross-crate moves; check/apply parity of the stranded-sibling finding)

All paths are relative to `packages/tddy-code-restructuring/` unless they start with `docs/` or `.config/`.
Read against `master` at `4a5c42b1b` (2026-10-09). Every claim below was checked against the code; where
a backlog entry was stale, it says so.

### E2.1 — The grouped-`use` refusal in the moved file's own paths (CONFIRMED OPEN)

- `src/crate_move/header.rs:67` `repointed_header` walks the survey grouped by `use` tree: leaves of one
  tree share a `site` (`header.rs:87-91`, `chunk_by` on `!in_body && site ==`). Each leaf gets a `Reach`
  (`header.rs:139-171`), then `rewrite_of` (`header.rs:177-222`) writes **one** edit: the tree's
  *prefix* (`written_prefix`, `header.rs:248-258`) replaced by a single new prefix.
- The refusal is `one_use_per_path` (`header.rs:238-244`), raised at two places:
  `header.rs:198-200` (a leaf's rewritten path no longer ends with its tail after the prefix) and
  `header.rs:205-206` (leaves disagree on the new prefix). The doc at `header.rs:63-66` says it is by
  design: "one prefix is all a group has".
- The refusal is reached from **both** `apply` (`src/crate_move/cluster.rs:129`, inside
  `resolve_cluster`) and static `check` (`src/crate_move/cluster/stranded.rs:141`, inside
  `paths_naming_the_origin`), so `check` already refuses the same group `apply` does — the parity
  of the refusal itself is not the problem; the refusal is.
- Pinned today by `tests/move_paths_acceptance.rs:244-262`
  `a_use_group_whose_members_land_in_different_crates_is_refused_with_the_fix` (live rust-analyzer,
  `.config/rust-e2e.filterset:63`). Its fixture is `use crate::{records::Id, OriginOwned};`, where
  `OriginOwned` stays in `origin` — once the group is split, that move is still refused, but by the
  dependency-cycle refusal (`src/crate_move/refusals.rs:25-40`), so the test's assertion changes shape
  rather than disappearing.

### E2.2 — Rule S already exists, one layer up (`repoint_facade_imports`)

- `src/backends/rust/repoint_facade/group.rs:20-78` `split_or_reprefix(statement, leaves)`:
  Rule P (`group.rs:64-70`, every leaf agrees → prefix replaced in place) and Rule S
  (`group.rs:72-77`, kept members first under the old prefix, each lifted member its own
  `<visibility> use <path>;`). Name preservation (`as <old>`) at `group.rs:201-211`; aliases and globs
  in `Member::parse` (`group.rs:118-144`).
- A nested member whose leaves need two prefixes is refused, `group.rs:181-182`
  (`refusals::nested_member_reaches_two_crates`, `repoint_facade/refusals.rs:73-78`). Its second half
  is pinned by `tests/repoint_facade_imports_acceptance.rs:421-440`
  `a_nested_group_member_is_lifted_whole_when_its_leaves_agree_and_refused_when_they_do_not` (asserts the refusal names
  the member `mix`), so flattening rewrites that test. (Corrected after the PRD review: a grep for the refusal's text
  missed it because the test asserts on the member name.)
- The statement-level wiring around it — finding the statement span (`use_statements`,
  `src/backends/rust/item_move/text.rs:116`), the `attribute_above_a_split` refusal
  (`repoint_facade.rs:203-205`, `repoint_facade/refusals.rs:64-70`) and re-indenting lifted lines
  (`repoint_facade.rs:208-213`) — is in `src/backends/rust/repoint_facade.rs:195-213`.
- **Layering problem the brief does not mention.** `group.rs` depends on
  `backends::rust::item_move::sites::members_of` (`sites.rs:392`, `pub(in crate::backends::rust)`) and
  `item_move::text::split_use` (`text.rs:158`, same visibility); `use_statements` too (`text.rs:116`).
  `crate_move` has **no** dependency on `backends` today (`grep backends src/crate_move` is empty), and
  `backends::rust` depends on `crate_move` (`repoint_facade/rewrite.rs:11-14`). "Reuse group.rs" from
  `crate_move/header.rs` by calling up would add a `crate_move → backends::rust` edge and a new cycle —
  exactly what the later crate split (stack 2) must not inherit. The reuse has to go the other way:
  the text-only group logic (`split_or_reprefix`, `Member`, `members_of`, `split_use`, the statement
  span finder) moves **down** into `crate_move` (e.g. `crate_move/use_group.rs`), and
  `repoint_facade` imports it from there.
- The leaf type differs: `repoint_facade/rewrite.rs:19-29` `Rewrite { written, defined_at, line,
  split_from_group }` (`pub(super)`) versus the header's `SurveyedPath` + `Reach`. The shared function
  needs a two-field leaf (`written`, `rewritten`) both callers build.

### E2.3 — A path through an in-crate glob facade to a co-moving member reads as staying behind (CONFIRMED OPEN)

- `src/crate_move/header.rs:142-144` tests co-movement with `travels_with` on `path.resolved` only.
  `resolved` is the written path rooted at the origin (`src/crate_move/survey.rs:100`);
  `defined_at` is the same path after `reexports::followed` (`survey.rs:101`,
  `src/crate_move/reexports.rs:36-53`), which **does** walk an in-crate glob
  (`reexports.rs:130-136`).
- R6 shape: `crate::connection_service::SeededAgentClones` where `connection_service.rs` has
  `pub use seed_codebase::*;` (verified at `3c9761ae3^:packages/tddy-session-lifecycle/src/connection_service.rs:207`).
  `resolved` = `origin::connection_service::SeededAgentClones` (no member matches);
  `defined_at` = `origin::connection_service::seed_codebase::SeededAgentClones` (member
  `connection_service::seed_codebase` matches). Today the leaf falls through to `header.rs:166-170`:
  `names = origin`, `edge_back = true` → the "stays behind" finding and the cycle refusal.
  R8's `split_start::SplitStartFailure` (`pub use split_start::*;`, same file `:324-325`) and R9's
  member names through the facade are the same root cause.
- The body-path finding already does it right: `src/crate_move/preconditions.rs:126-135`
  `module_left_behind` tests `travels_with` on `defined_at`. So the fix is a one-site change in
  `reach` (test `resolved`, then `defined_at`), and the landing is computed from the `defined_at` rest:
  `crate::seed_codebase::SeededAgentClones`.
- This fix is what turns the R6 group into a **different-qualifier** group (E2.1): the three leaves
  land on `crate` / `crate` / `crate::seed_codebase`. So E2.3 without E2.1 still refuses; E2.1 without
  E2.3 still says "stays behind". They ship together.
- Not reproduced: R8's `AttachmentProgressSink` reached through
  `pub(crate) use tddy_session_files::attachment_progress::*;` (`3c9761ae3^:…/connection_service.rs:320`),
  a glob into **another** crate. `follow_absolute` should resolve it to `tddy_session_files`, which is
  no edge; why it was refused is unknown. Wave 2 adds one probe fixture for it; if it reproduces it is
  this node's (same function), otherwise it is reported as not reproduced.

### E2.4 — `pub(in crate::origin_module)` is read as a path naming the origin (CONFIRMED OPEN, and worse than recorded)

- The survey reads `pub(in crate::connection_service)` as a body path: the token walk
  (`src/crate_move/source_scan/sighting_walk.rs:107-121`) treats `in` as a one-segment path (dropped,
  `< 2` segments) and then reads `crate::connection_service` as a two-segment body path. Nothing marks
  it as a visibility restriction (`docs/ft/coder/rust-code-restructuring.md:928`: "`pub(in …)`
  visibility is not interpreted").
- In `reach` it resolves to `origin::connection_service`; when that module is not co-moving, the
  result is `names = origin`, `edge_back = true` (`header.rs:166-170`) → listed in `origin_paths` →
  `refuse_a_dependency_cycle` (`refusals.rs:25-40`) refuses whenever the origin keeps naming the module.
- **Worse:** when the origin does *not* keep naming the module (`reexport: none` and no caller), the
  refusal does not fire and `rewrite_of` writes `pub(in origin::connection_service)` — an extern path
  in a visibility restriction, which never compiles. Read from code, not reproduced.
- `stays_behind_through_a_body` does not fire on it (`preconditions.rs:131-133` needs a module *and* an
  item segment), and the stranded finding (header-only, E2.5) does not either — so today this is an
  `apply`-only refusal.
- Where the restriction names a co-moving module (the moved module itself, or an ancestor in the
  set), the existing `travels_with` branch already rewrites it correctly to `crate::<landing>`.
- What the restriction should become when it names a module that stays: the module no longer exists in
  the destination crate. The narrowest valid respelling inside the destination is `pub(crate)`;
  whether the origin still needs it cross-crate (→ `pub`) is cross-crate widening, which is node 7's
  (`move-widen`) responsibility over `pub(crate)` items. The R9 hand edit (21 × `pub(in
  crate::connection_service)` → `pub`) is consistent with that composition.

### E2.5 — The stranded-sibling finding reads only the top-level `use` header (CONFIRMED OPEN, entry accurate)

- `header.rs:27-37` field `header_origin_paths` with `TODO(check-parity-header)`; filled at
  `header.rs:106-110` only for `!in_body` paths on a top-level `use` line (`header_lines`,
  `header.rs:74-77`, from `use_declarations`, `header.rs:302-332`, which skips indented lines).
- Its one reader: `src/crate_move/cluster/stranded.rs:132-143` `paths_naming_the_origin` returns
  `header.header_origin_paths`. `apply`'s `refuse_a_dependency_cycle` reads `origin_paths` (every path
  outside `cfg(test)`, bodies and nested `use` included).
- Gaps this leaves, concretely: a `use` inside a function or inline non-test `mod`; a body path to an
  item of the origin's **crate root** (`stays_behind_through_a_body` skips root items,
  `preconditions.rs:131-133`).
- **A test pins the gap.** `tests/check_precondition_parity.rs:497-521`
  `a_body_path_to_an_item_at_the_crate_root_is_no_finding` asserts `unrunnable_moves` (which includes
  `siblings_left_behind`, `src/crate_move/preconditions.rs:29-35`) reports nothing for a `reexport:
  glob` move whose body calls `crate::helper()`. `apply` refuses exactly that plan —
  `tests/move_paths_acceptance.rs:206` `a_body_path_to_an_item_the_origin_defines_is_an_edge_back_and_refuses_the_move`.
  Widening the finding flips this test; its doc comment ("the header pass owns root items") is the
  check/apply disagreement the entry describes.
- Double reporting: a module that fails `move_preconditions` (incl. `stays_behind_through_a_body`) is
  dropped from the stranded pass (`stranded.rs:180-187`, `.ok()?`), so a body path to `module::item`
  is reported once, by the body finding. The widened finding adds root-item bodies and nested `use`s.
- The finding's remedy text (`stranded.rs:105-119`: "move them in one `move_cluster_to_crate` … what
  those paths reach in `also`") is wrong for a root item, which is not a module that can join `also`.
- Static `check` calls it at `src/runner/entry_points/check_entry_points.rs:261`.

### E2.6 — A caller re-point spliced inside a grouped `use` (09-09 first-cross-crate item 4 — CONFIRMED OPEN by reading, `reexport: none` only)

- Caller rewrites: `src/crate_move.rs:233-254` (`surveyed`). `header::written_path_at`
  (`header.rs:344-364`) walks back from the reported identifier over `seg::` and stops at `{`. For
  `use crate::{connection_service::agent_roster, livekit_rooms_stream::RoomRoster, spawn_worker};` the
  written path is `livekit_rooms_stream::RoomRoster`; `header::repointed` (`header.rs:378-388`) gives
  `dest::livekit_rooms_stream::RoomRoster`; the span is replaced in place → the recorded
  `use crate::{…, dest::livekit_rooms_stream::RoomRoster, …}`.
- Only applied when `reexport == None` (`src/crate_move/cluster.rs:140-144`); with `glob` the facade
  keeps callers compiling and none is rewritten.
- **Second defect on the same path:** `resolve_cluster` builds caller changes **per member**
  (`cluster.rs:126-145`) and `MergedChanges::add` (`cluster.rs:227-232`) concatenates edits. A caller
  group naming two members (`use crate::{a::X, b::Y};`) gets one span edit per member, and once the fix
  replaces a whole statement, two overlapping statement edits. Caller rewrites have to be collected
  across the set and turned into statement edits once.
- The fix is the same Rule P / Rule S over the caller's statement: Rule P when every leaf agrees
  (`use crate::{a, b}` → `use dest::{a, b}`), Rule S otherwise
  (`use crate::{connection_service::agent_roster, spawn_worker};` + `use dest::livekit_rooms_stream::RoomRoster;`).

### E2.7 — Out of this node's reach, recorded so the wrap narrows rather than deletes

- `src/crate_move/test_binary.rs:801-822` `defining_home` refuses `origin::{…}` / `origin::*` in a
  **test binary** move (`move_test_binary_to_crate`), a separate, text-only reader with no survey.
  Not met on any `#carve` run; the file is node 15's (`oversized-files`) to split. Deferred.
- Hand-split file, R6 "third hand edit": a path the moved file writes through a *third* crate's
  re-export (`tddy_sandbox_runner::ExecuteToolResponse`) puts that crate in the destination manifest.
  `reexports::followed` deliberately leaves non-origin paths alone (`reexports.rs:41-43`), and
  `repoint_facade_imports` documents the same limit. Changing it rewrites a path the author wrote
  explicitly — a policy change, not a bug. Deferred (new todo).
- Hand-split file, R8 `hooks_and_urls` left out of the cluster and R9 item 4 (`family_proto_bridge`
  joins the cluster): plan corrections, not engine work.
- Hand-split file, R9 item 3 `mod x;` → `pub mod`, `pub(crate) mod` → `pub mod`: needed only because the
  hand re-spellings went through `crate::connection_service::<member>::…`; with E2.3 the engine resolves
  the facade itself. Restricted `mod` declarations in the move are node 5's (`move-children`).
- `item_move/sites.rs:335` (nested group refused by `move_item`) is a same-crate op — not this node.

### Reusable pieces and harness

- Library level, no server: `resolve_cluster` and `crate_move::{ModuleReferences, ItemReferences,
  Reference}` are public (`src/lib.rs:27-31`, `src/crate_move.rs:73`, `:96-102`), so a new test binary
  can drive a whole cluster move over a fake reference set. The in-crate fake to copy is
  `AKnownReferenceSet` (`src/crate_move/cluster.rs:345-396`) with fixture builder `AWorkspace`
  (`cluster.rs:281-338`) and `applied(...)` (`cluster.rs:423-438`).
- Static findings: `tests/cluster_move.rs:108-114` `stranded_by` (`siblings_left_behind`), and
  `tests/check_precondition_parity.rs:82-89` `unrunnable_in`.
- Live: `tests/cluster_move_acceptance.rs` (registered, `.config/rust-e2e.filterset:46`) with
  `harness::{a_cluster_move_of, performing, assert_compiles}` (`tests/harness/mod.rs:762`, `:381`, `:908`).
  One end-to-end test reproducing the R6 shape is enough; everything else is library level.
- Collisions (textual, not behavioural): `header.rs` imports `test_binary::segment_length`
  (`header.rs:12`), which node 15 moves; `stranded_siblings` (70 lines) and `resolve_cluster`
  (111 lines) are on node 16's function-size list — this node must not grow them.

# Initial discovery — #reshape 1/19 `widen-same-crate`

Companion to [the changeset](2026-10-09-reshape-widen-same-crate.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — widen-same-crate (member widening for `move_item`, tree reach for `reparent_module`, emptied directories)

Read on 2026-10-09 against `4a5c42b1b` (master). All paths are under `packages/tddy-code-restructuring/`
unless they start with `docs/` or `.config/`.

### E2.1 — `move_item` sees module-level items only (claim of `2026-10-04-…-move-item-does-not-widen-fields-or-impl-members` holds)

- `src/backends/rust/item_move/outline.rs:55-90` `run_covering` builds `Run.items` from the **root** symbols
  of the outline only, and keeps a symbol only when its name is an identifier (`:75`), so an `impl Counter`
  block is in the moved lines but never in `items`; its `children` (the methods) are never read.
- `outline.rs:150-169` `left_behind` is the same, for the root symbols outside the run.
- `src/backends/rust/item_move.rs:192-225` `reached_by_the_moved_code` asks `textDocument/references` only
  for those root `left` items whose names `text::identifiers_in` finds in the moved lines.
- `src/backends/rust/item_move/assemble.rs:206-276` `visibilities` widens the moved root items (to cover
  `users_of`, `:280-300`) and the reached root items (to cover the destination). Nothing reads a field or
  a method. **Confirmed: no E0616/E0624 handling anywhere in `item_move/`** (`grep -rn "E0616\|E0624" src`
  finds only prose).
- The outline does carry the members: rust-analyzer reports an `impl` as `Object` (19) with `Method`
  children (`src/backends/rust.rs:64-73`), and a struct with `Field` children; `rust.rs:2648` and
  `rust.rs:2739-2743` already walk `children` for `extract_module`.
- `rebase::edits` (`item_move/rebase.rs:36-68`) already respells a **relative** `pub(super)`/`pub(in …)` on a
  moved field or method so it keeps its meaning (`visibility_edit`, `:89-96`). The gap is the **private**
  member (no keyword) — private means "the module it is written in", which a move changes — and a
  member of a left-behind type the moved code reaches.

**Reusable:** `Scope` (`item_move/scope.rs:12-82`: `parse`, `widened_to`, `spelled_in`) answers every
"how wide" question; `sites_of` (`item_move.rs:144-186`) turns a list of `(name, position)` into `Site`s and
works for a member's position as well as an item's; `users_of` (`assemble.rs:280-300`) turns sites into
user modules but filters by **name only** (`site.name == name`), so a field `name` and a moved `fn name`
would mix — a member survey needs its own key (type + member). `enclosing_modules`
(`item_move/text.rs:173`) and `module_of_file` (`item_move/sites.rs:84`) place a site.

**Not reusable as is:** `outline::visibility_edit` (`outline.rs:202-222`) refuses whenever the text between
the visibility and the name is empty (`:209-214`, "keyword on a line above its name"). For a field that is
the normal case — `count: u32` and `pub(super) count: u32` both have nothing between visibility and name —
so a field needs a member-aware edit (insert or replace the visibility directly before the name). A method
(`fn bump`) passes as is.

**Not seen by the outline (to verify in the first push):** tuple-struct fields (`struct Id(u32)`, read as
`self.0`). rust-analyzer's file structure lists record fields; if it does not list tuple fields, a split
tuple struct stays a compile-gate failure and the PRD says so.

### E2.2 — `reparent_module` widens only the declaration (claim of `2026-10-04-…-reparent-module-does-not-widen-…` holds)

- `src/backends/rust/module_reparent/visibility.rs:32-75` `landing` computes the `mod` declaration's new
  visibility (the same `starts_as` → `widened_to(new)` → widen to `users_of` rule `move_item` uses). It is
  the only visibility the operation writes.
- `src/backends/rust/module_reparent/assemble.rs:176-217` `rebase_the_moved_files` passes
  `travelling: Some(&old)` (`:203`), and `rebase::edits` then **skips every visibility span** in the moved
  files (`item_move/rebase.rs:47-52`: `if modules.travelling.is_some() || … { continue; }`). That is the
  root of all three sub-items:
  - `pub(super) fn materialize` at the top of the moved module meant `host` and now means `split`;
  - `pub(in crate::host::attachments)` names a module that no longer exists (rustc: `E0433`/`E0742`);
  - nothing surveys the old parent's private items the tree names.
- No outline of the old parent is read today: `callers_of_the_module` (`module_reparent.rs:90-111`) opens
  the old parent (`did_open` + `ensure_indexed`) and asks references on the `mod` name only. The outline is
  one `settled_outline(&uri)` away on a document already open.
- Existing coverage of the reproduced case is a **public** `host_name` (`tests/reparent_module_acceptance.rs:42`,
  `THE_HOST_ITSELF = "pub fn host_name() …"`), which is why `rebases_a_super_path_that_named_the_old_parent`
  (`:204-232`) passes while the private case fails with `E0603`.

**Bound on the survey (new finding):** an item of an ancestor module `A` that is visible to the moved tree
stays visible after the move whenever `A` is also an ancestor of the new location. So only the old parent
and the ancestors strictly below the **common ancestor** of the old and new parents can hold an item that
needs widening. For `host::attachments` → `split::attachments` that is `host` alone.

**A tree visibility needs no server.** Read each `pub(…)` span in a moved file at its old module
(`Scope::parse(span, old_module_of_that_file)`, inline modules included via `enclosing_modules`, as
`rebase::modules_at` already does). If the scope lies inside the moved tree, translate its prefix
(old tree path → new tree path) and respell; if it reaches outside the tree, keep everything it covered and
widen it until it is legal where the file now sits: `scope.widened_to(new_module_of_that_file)`. Every caller
that could see it before still can, which is `move_item`'s `starts_as` rule (`assemble.rs:223-228`).

### E2.3 — emptied directories (claim of the 10-04/10-05 sub-items holds; it is not reparent-specific)

- `src/apply.rs:18-39` applies `Create`, then `Change`, then `Rename`; `git_move` (`:158-163`) creates the
  destination directory and runs `git mv`. Nothing removes a directory: `grep -rn remove_dir src` returns
  nothing.
- Every operation that emits `FileEdit::Rename` leaves the vacated directory: `reparent_module`
  (`module_reparent.rs:76-81`), the cross-crate moves and `move_test_binary_to_crate` (`crate_move/`). So the
  fix belongs in `apply.rs`, the module whose doc says it is "the only component that touches the
  filesystem" (`apply.rs:1`), and fixes every one of them at once.
- Rollback is safe: `PreImage::restore` (`src/journal/group.rs:36-53`) recreates the parent directory with
  `create_dir_all` before rewriting a file, and `git_move` does the same, so a removed directory is
  recreated by any rollback or re-run.
- Related limit with the same shape, listed in the feature doc but in no todo:
  `docs/ft/coder/rust-code-restructuring.md:859` "A rolled-back group leaves an empty directory it created a
  file in" — `PreImage::restore` with `contents: None` removes the file only (`group.rs:47-51`).
- Existing assertion gap: `moves_a_directory_shaped_module_with_its_children`
  (`tests/reparent_module_acceptance.rs:165-200`) says "nothing is left in the old directory" but asserts only
  that `src/host/attachments/staging.rs` is gone; the empty `src/host/attachments/` is still there.

### E2.4 — stale sub-items of the claimed entries

- `2026-10-04-restructure-reparent-module-first-cut-limits.md` item 1 ("the PRD and `plan-schema.md` should
  say which"): already done — `.agents/skills/code-restructuring/references/plan-schema.md:220` and
  `docs/ft/coder/rust-code-restructuring.md` § Same-crate moves both say `pub use <new parent>::<module>;`.
- Same entry, item 2 (the facade test asserts `src/split.rs` is byte-identical): already fixed —
  `tests/reparent_module_acceptance.rs:286-297` asserts `ends_with(A_SPLIT_THAT_IMPORTS_IT)`.
- `2026-10-05-…-limits-found-moving-lifecycle.md` item 2 (a `pub use` chain that re-exports the destination
  is not followed by `reach.rs`): `item_move/reach.rs:103-140` does follow `mod` declarations only, but the
  entry says "none of which that run hit", there is no reproduction, and re-pointed callers are written
  with the canonical `crate::<destination>` path (`assemble.rs:310-317`), so it is unclear which caller would
  pass through a re-export. Needs a reproduction before it can be planned.

### E2.5 — size budgets this node must not worsen

- `item_move/assemble.rs` is 507 lines (node 15 `oversized-files` claims it) and `assemble::visibilities`
  (`:206-276`) is 71 lines (node 19's list). New member and tree widening goes into **new files**
  (`item_move/members.rs`, `module_reparent/tree_reach.rs`, `module_reparent/tree_visibility.rs`), with at
  most a call and a field added in `assemble.rs` / `module_reparent/assemble.rs`.
- `src/apply.rs` is 240 production lines; node 10 (`apply-robust`) also edits `git_move` (`:158-163`) for its
  `ls-files` preflight. Textual collision only.

### E2.6 — test harness, fixtures and a registration gap

- Live suites build a one-package `app` with `tests/same_crate/mod.rs::an_app_holding`, anchor with
  `the_anchor_over` (the real `restructure anchors --items`), and apply with `moving_items` /
  `reparenting_module`; oracles are `harness::assert_compiles`, `assert_compiles_with_its_tests`
  (`tests/harness/mod.rs:908-919`) and `assert_lints_clean` (`:2673`). The run's widenings are on
  `RunSummary` (reported through `report_visibility`, `src/runner/entry_points.rs:218-228`).
- Library-level tests without a server: `item_move::assemble` and `module_reparent::assemble` are
  "a function of texts" (both module docs say so), and `apply.rs` has tempdir + `git init` unit tests
  (`apply.rs:211-370`) — the pattern for the directory removal.
- **Registration gap (pre-existing):** the nine same-crate suites that drive a live rust-analyzer —
  `move_item_acceptance`, `move_item_beyond_the_basics_acceptance`, `move_item_creates_module_acceptance`,
  `move_item_into_an_existing_module_acceptance`, `move_item_of_an_item_the_destination_imports_acceptance`,
  `move_item_outside_facade_acceptance`, `reparent_module_acceptance`,
  `reparent_module_beyond_the_basics_acceptance`, `reparent_module_through_the_old_parents_import_acceptance`
  (and `same_crate_deep_check_acceptance`) — are in **neither** `.config/rust-e2e.filterset` nor the
  `rust-analyzer` test group (`.config/nextest.toml:88-106`), although each file's header says the group
  enforces one server at a time. They run in the `Rust tests` leg, against `.config/nextest.toml:18-25`'s rule.
- `check --deep` prints a resolution's notes but drops its `report` (widenings): `src/runner/rehearsal.rs:69`
  copies `resolved.notes` only, and `check_entry_points.rs:295-303` prints notes. Node 7 (`move-widen`) owns
  "report every widening in rehearsal/`check --deep`"; this node's widenings travel in `Resolution.report`
  and appear there once node 7 lands.

# Initial discovery — #reshape 6/19 `methods-leave-type`

Companion to [the changeset](2026-10-09-reshape-methods-leave-type.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — methods-leave-type (`read_fields_through`, `retarget_impl`'s `leave_delegator`, dead wrappers)

Checked against the tree at `4a5c42b1b`. Paths are relative to `packages/tddy-code-restructuring/src/`
unless noted. Both claimed entries were read in full:
`docs/dev/todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md`
(the **state-parameter** entry) and
`docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md`
(the **delegator** entry).

### 1. Status of each claim

| Claim | Status on `4a5c42b1b` | Evidence |
|---|---|---|
| state-parameter: an operation that turns `self.<field>` into `<state>.<field>` across a range and inserts `let <state> = <builder>;` | **MISSING**. No `RefactorKind` does it (`plan/refactor_kind.rs:14-162`, 25 kinds; `backends/rust.rs:75` `SUPPORTED: [RefactorKind; 25]`) | `docs/repoint-call.md:108-111` says it stays open |
| delegator entry, receiver half (`repoint_call`) | **FIXED** (#594): single form and bulk form | `backends/rust/repoint_call.rs:1-8`, `repoint_call/sites.rs:28-85` |
| delegator entry, delegator half (`variant: "leave_delegator"` + `expr`) | **PARSED, THEN REFUSED.** The codec accepts the variant (`plan/codec/retarget_fields.rs:91-95,106`) and `expr` on `retarget_impl` (`plan/codec/signature_fields.rs:27-32`). The backend refuses both as a static finding (`backends/rust/retarget_impl.rs:29-33`, `deferred_delegator` `:41-48`) and as `UnsupportedOp` in `apply` (`:61-66`). The marker is `TODO(retarget-impl)` at `:38-40` | see below |
| delegator entry, P9 (variant/expr pairing) | **NOT ENFORCED** at parse time. `refuse_a_field_it_does_not_define` (`retarget_fields.rs:84-103`) checks only that the variant is `leave_delegator`; a `variant` without `expr`, or an `expr` without `variant`, gets past the codec and is caught only by `deferred_delegator` | `retarget_fields.rs:84-103` |
| delegator entry, "report any wrapper with no caller left" | **Generic half already exists**: after an apply, the tidy prints `warning remains: <file>:<line>: function `x` is never used` for every warning left in a touched file (`runner/tidy.rs:525-536`, pinned by the unit test at `:698-711`). That covers a private host wrapper that lost its last caller. It does **not** cover a `pub` method of a library crate, where rustc never warns, or a file the run did not touch. **Delegator half missing** (there is no delegator yet) | `runner/tidy.rs:525-536` |

### 2. The delegator was already designed, and cut

The `#sharpen` 6/8 changeset (`git show 7302f2862:docs/dev/1-WIP/2026-10-05-sharpen-retarget-impl.md`)
fully specified the delegator as milestone M4, then cut it at the M3 decision point (Decision O2):
`retarget_impl/` had reached ~895 production lines. The cut design, which this node can take over:

- schema (line 164, 172-173): `"variant":"leave_delegator","expr":"self.roster()"`. `expr` is one
  `syn::Expr` checked by the existing `one_expr` (`plan/rust_syntax.rs:35`);
- P9 (line 192): the variant/expr pairing; S7 (line 204): a pattern parameter, an associated const or
  type, or a `const fn` cannot be forwarded;
- the forwarding method (lines 258-277): the signature is copied byte for byte from the visibility to
  the body's `{`, outer attributes are copied but doc comments are not, and the body is
  `<expr>.<name>(<argument names>)` plus `.await` for an `async fn`. An associated function with no
  receiver forwards as `New::<name>(…)`;
- `verify` R3 (line 289): a gained `fn` signature equal to one the ref has, paired with the gained
  forwarding statement;
- tests 31-35 (lines 396-402): the delegator acceptance binary (live), the P9 plan-line test, and the
  R3 verify test. All of them were deleted when M4 was cut.

**Gap in the cut design.** It covers only whole-block geometry: "`Old: delegators`, then `New: M`"
(line 221). It does not say where the delegators go when the moved members are a proper subset, which
is a `[Old: P] [New: M] [Old: A]` split (`docs/retarget-impl.md:35-43`). The PRD has to settle this
(Decision F5).

### 3. What `retarget_impl` already computes that the delegator reuses

- `backends/rust/retarget_impl.rs:79-148` `retarget_impl()` is **70 lines**, on node 19's list of
  functions over 60 lines (whole-work discovery, Exploration 3). This node must not make it longer. The
  delegator goes in through a new child module and the existing `rewrite::Layout`
  (`retarget_impl/rewrite.rs`), not through new lines in that function.
- `:105-109` `moved`, the `(name, text)` of each moved member, with attached trivia (`member_text`,
  `:185-189`). That is everything the signature copy needs.
- `:120` `self.sites_of(…)` already returns **every** reference to every moved member, in **every**
  file the server knows (`backends/rust/item_move.rs:144-189`). `:121-125` keeps only the references
  inside the moved range. Any other reference is a caller that the delegator would keep compiling, so
  "this delegator has no caller left" is `sites` minus `inside`. **That costs no extra server request.**
- `:145` `notes: Vec::new()`. `Resolution.notes` (`edit.rs:71-75`) is printed by `apply`
  (`runner/entry_points.rs:224`) and by `check --deep` (`runner/entry_points/check_entry_points.rs:301`),
  so a note is all the dead-delegator report needs.
- `retarget_impl/fields.rs:49-93` S4 refuses a moved member that reads a field the new type lacks. It
  runs before the delegator would be built, so a delegator never forwards to a member S4 refuses.

### 4. What `read_fields_through` can reuse, and what it cannot

| Need | Reusable | Where |
|---|---|---|
| Plan fields | `name` (binding), `expr` (the value), `variant`, all existing on `RefactorOp` | `plan.rs:202,210,252` |
| Adding a **new** `RefactorOp` field | costs one literal edit in each of **22 places across 16 files** (counted by `callee: None`: `src/crate_move.rs`, `crate_move/cluster.rs`, `registry.rs`, `runner/budget.rs`, `backends/rust/signature_rewrites.rs`, 11 test files). `RefactorOp` has no `Default` | `plan.rs:192-272` |
| `expr` validation | `one_expr`, run for every op that carries `expr` | `plan/codec.rs:441-443`, `plan/rust_syntax.rs:35` |
| Which op may carry `expr` | the allow-list must admit the new op | `plan/codec/signature_fields.rs:27-32` |
| Static per-op refusals | one `refuse_…` module per op, called at the end of `parse_op` | `plan/codec.rs:445-448,452-459` (`parse_op` is 206 lines and node 16 splits it: one added call line, no added logic) |
| Comment/string masking for a lexical scan | `masked_to_code` | `backends/rust/early_return.rs:271` |
| A trial text shown to the server | `did_change`, the pattern `imports.rs:56,72,269` and `signature.rs:136` already use | `backends/rust.rs:977` |
| "Is this name unresolved in the trial text?" | `unresolved_names` (semantic tokens) | `backends/rust.rs:1900-1917` |
| Type of a position | `textDocument/hover` is already requested (`inference_ready_at` `backends/rust.rs:992-998`, `readiness.rs:81,165`), but **nothing parses hover contents** (no `contents` reader anywhere in `src`) | new |
| `textDocument/definition` | **not used anywhere** in `src` | not needed if hover is used |
| `syn` reader of `self.<field>` reads | `retarget_impl/fields.rs:137-187` (`Reads` visitor). It works on whole members only and returns no offsets: without `proc-macro2`'s `span-locations` feature, `syn` gives no byte positions. That feature is enabled only in `tddy-code-analysis/Cargo.toml:15`, not here, and turning it on would add a dependency | not reusable for edits |
| Declaration of the new type | `retarget_impl/fields.rs:29-43` reads a type **of the same package only** (`package_of` + `find_module`). The state type in the todo (`tddy_session_agents::AgentRosterState`) is in **another crate**, so this reader cannot type it | not reusable for the state type |

**Partial workaround that already exists.** The `repoint_call` single form
(`repoint_call/single.rs:14,58-62`) can rewrite `self.session_agent_rosters.entry(…)` into
`state.session_agent_rosters.entry(…)`, because the old callee holds no call. Both example sites in
the state-parameter entry are of that shape. It still takes one plan line per site, does not insert
the `let`, does not reach a field that is read without being called (`&self.config`), does not drop
the `&` that turns into `needless_borrow`, and does not check that nothing in the range still names
`self`. So `read_fields_through` is still needed, but the todo understates what already works.

### 5. Challenge: the brief says node 18 consumes this node's ops, and as the todo specifies them it cannot

The brief's real edge `6→18` says `backend-session` makes `RustBackend` operations free functions
"using node 6's ops". The state-parameter entry's operation **refuses any `self.<method>(…)` in the
range**. `RustBackend` operation bodies are almost entirely method calls on `self`
(`self\.<ident>(` against every `self\.<ident>`):

| file | `self.m(` | `self.f` |
|---|---|---|
| `backends/rust/retarget_impl.rs` | 6 | 0 |
| `backends/rust/signature.rs` | 5 | 0 |
| `backends/rust/item_move.rs` | 9 | 1 |
| `backends/rust/module_reparent.rs` | 5 | 1 |
| `backends/rust/repoint_call.rs` | 1 | 1 |

For example, `retarget_impl.rs:87-120` calls `self.retarget_range`, `self.start`, `self.did_open`,
`self.ensure_indexed`, `self.settled_outline` and `self.sites_of`. The field-only operation would
refuse almost every range node 18 needs. What does serve node 18 is a **self-rebinding** form:
`let backend = self;`, after which **every** `self` in the range, method calls included, is written
`backend`. Then `extract_method` writes a free `fn …(backend: &mut RustBackend, …)`, because
rust-analyzer's "Extract into function" produces a free function only when the range names no `self`
(state-parameter entry, "What is missing"), and the method that stays behind forwards to it. This needs
no type check: the binding is the same object. So it is the cheaper of the two modes (Decision F2).

### 6. `verify` today and the two new shapes

- `verify/retarget.rs:27-32` `Declared { retargets, repoints }`. `verify.rs:204-208` runs the passes
  in this order: visibility, `retarget::account` (R1/R2), `repoint::account` (R-call), re-point key,
  reflow.
- **Field rebinding** produces `self.f.m(…)` → `state.f.m(…)` and one gained
  `let state = <expr>;`. R-call (`verify/repoint.rs:9-12`) needs `from` to be followed by `(`, so it
  pairs `self.f.m(` but not `&self.config` or `self.n + 1`. R1 (whole-identifier `OLD`→`NEW`,
  `verify/retarget.rs:8-11`) pairs both. `Retarget::from_str` (`:44-63`) **accepts `self=state`**,
  since `self` passes its identifier check, so `--retarget self=state` pairs the rebound statements
  today, though with a misleading name. Nothing excuses the gained `let`.
- **Delegator** produces a duplicated signature statement plus one forwarding statement. Statements
  are a multiset (`verify.rs:175-201`; `verify/statements.rs:13-30` joins wrapped lines and drops bare
  braces), so R3 can be written over the multiset with no knowledge of adjacency.
- Carriers for a new declaration kind, if one is chosen: `RestructureVerifyArgs`
  (`restructure_args.rs:249-250`), `runner/options.rs:87-90,141-142,206-214`,
  `runner/comparison.rs:31`, proto `tddy-index-daemon/proto/code_index.proto:280-288` (fields 3 and 4
  are taken, so 5 would be next), `tddy-index-daemon/src/cli.rs:387,685-713`,
  `tddy-index-daemon/src/queries.rs:290`, `tddy-tools/src/index_client.rs:375`.

### 7. Wiring and collisions

- New op wiring is one `SUPPORTED` entry (`backends/rust.rs:75`, 25 → 26), one `check` arm
  (`:1131-1136` pattern) and one `resolve` arm (`:1303-1311` pattern). Node 17 shrinks that file, so
  the collision is textual only.
- `plan/refactor_kind.rs:150-161` gets one variant. `plan/codec.rs:445-459` gets one call and one
  `mod`, in the same function node 16 splits.
- Docs that count operations or describe the delegator as refused: `docs/ft/coder/rust-code-restructuring.md:321,420-446`,
  `.agents/skills/code-restructuring/references/plan-schema.md:131,291-326`, `.agents/skills/code-restructuring/SKILL.md:67`,
  `packages/tddy-code-restructuring/docs/retarget-impl.md:45-62`, `docs/repoint-call.md:108-113`.
  These are edited at wrap, not now.
- No code issue in `docs/code-issues/` names `retarget_impl` or this area, so there is no `Claimed by`
  conflict.

### 8. Test harness and fixtures

- **Live (rust-analyzer):** `tests/retarget_impl_acceptance.rs` (588 lines), registered in
  `.config/nextest.toml:102` and `.config/rust-e2e.filterset:72`. Helpers it already has:
  `a_retarget_op` `:42`, `a_crate_whose_host_reads` `:49`, `a_host_impl_of` `:57`,
  `the_anchor_over_members_of_host` `:63`, `retargeting` `:76`. The delegator cases belong in a **new**
  thin binary `tests/retarget_impl_delegator_acceptance.rs` (the name the cut M4 used), since the
  existing binary is already 588 lines. `read_fields_through` gets its own live binary,
  `tests/read_fields_through_acceptance.rs`. Both must be registered in the two `.config` files.
- **Fixtures:** `tests/same_crate/mod.rs:32` `an_app_holding` (one crate) and `:196`
  `an_app_over_a_kernel` (two crates, `app` depending on `kernel`). The second is what proves the
  state type can live in **another crate**, the real T3 shape.
- **Assertions:** `tests/harness/mod.rs:908` `assert_compiles`, `:916`
  `assert_compiles_with_its_tests`, `:2673` `assert_lints_clean` (`cargo clippy … -D warnings`, which
  pins the `needless_borrow` rule).
- **Library level, no server:** `tests/retarget_impl_plan_lines.rs` (P1-P8 style; P9 goes here), a new
  `tests/read_fields_through_plan_lines.rs`, `tests/verify_accounts_for_a_retarget.rs` (R3), and a new
  verify test for the rebind declaration. The forwarding-method text, the signature copy and the
  statement-boundary rule are functions of text, so they can be unit tests in their own modules.

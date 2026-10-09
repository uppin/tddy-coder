# Initial discovery — #reshape 13/19 `move-impl-members`

Companion to [the changeset](2026-10-09-reshape-move-impl-members.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — move-impl-members (moving a run of inherent `impl` members into another module of the same crate)

Read on 2026-10-09 against `4a5c42b1b` (master). All paths are under `packages/tddy-code-restructuring/`
unless they start with `docs/`, `.agents/` or `.config/`.

### E2.1 — The premise "extract_module refuses impl members" is only half true

The brief and the whole-work discovery say `extract_module` refuses to move `impl RustBackend` members
because rust-analyzer cannot put a `mod` inside an `impl`. The code says something narrower.

- **`extract_module` already moves a run of inherent `impl` members**, into a **new inline child module**
  of the file: rust-analyzer writes `mod x { use super::Gauge; impl Gauge { … } }`
  (`src/backends/rust/impl_seam.rs:13-21`). It is tested against a live server in
  `tests/impl_seam_acceptance.rs:36-152`: six cases pass, with a left-behind member calling the moved one
  through `self.`, `Self::`, the type, and a type alias. `impl_seam::with_method_calls_restored`
  (`impl_seam.rs:94-125`) undoes the assist's `self.modname::m()` rewrite.
- **What it refuses, and why:**
  - half of a **trait** `impl` (`impl_seam.rs:29-66`, `refuse_impl_sibling_references`). This is correct
    and stays refused here too.
  - a **residual placeholder inside an `impl`** (`src/backends/rust/placeholder_checks.rs:88-110`). The
    rename of `modname` did not reach a call written inside an `impl`. That is the "an `impl` body
    cannot hold a `mod`" wording quoted in `oversized-file-backends-rust.md` and in the #567 changeset
    (`packages/tddy-code-restructuring/docs/changesets/2026-10-04-signature-rewrites.md:40-43`). The
    lifecycle-destructure run hit the same refusal on a cold index and passed after a warm-up
    (`docs/dev/todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md:303-311`).
- **What it cannot do at all:**
  - **land in an existing module, or in a module that is not a new child of the file.** #567's members
    (`rewrite_signature`, `wrap_or_unwrap_return_type`, `offered_assist`) belonged in the existing
    `signature_rewrites` module. No operation can put them there: `extract_module` only creates a
    module, and `move_item` refuses a range inside an `impl` (`item_move/outline.rs:52-55` and
    `:108-118`, "a member of an `impl` cannot move alone … Move the `impl` block itself").
  - **widen by as little as the code needs.** Every private member it moves is widened to `pub(crate)`
    by the assist. `facade::impl_widenings` (`src/backends/rust/facade.rs:14-55`) reports the widening
    and deliberately never narrows it back. Leftovers § 1
    (`docs/dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md:24-28`)
    names this as one reason the `rust.rs` split was not done: about 25 methods would stay `pub(crate)`.
  - **work without an assist.** It depends on rust-analyzer's rename reaching every call. That rename is
    where the residual-placeholder refusal and the cold-index failure come from.

**Consequence for the design.** The new operation is not "the only way to move a member". It is the
deterministic, text-assembled way, informed by the server and needing no assist. It goes to **any**
module of the crate, existing or created, and widens no further than the callers need. That is what
`move_item` did for free items, and this operation reuses `move_item`'s pieces in the same way.

### E2.2 — `rust.rs` as it is today (what node 17 has to move)

`src/backends/rust.rs` is 5,566 lines. The first inline `#[cfg(test)] mod` is at `:3009`, so it has
**3,008 production lines** by the code-issue record's rule. That is also the record's own latest row,
and 2,755 of those lines are non-blank. The brief's 2,983 matches neither count.

| Block | Lines | Size | Movable by |
|---|---|---:|---|
| header, `struct RustBackend` (`:425`), `Assist` table, `assist_for`, `Placeholder` | 1–515 | ~515 | free items: `move_item` / `extract_module` (nodes 2, 3) |
| `impl RustBackend` #1 (`new` … `offered_assist`) | 516–1094 | 579 | **this op** (keep `new`) |
| `impl Drop` / `impl LanguageBackend` / `impl ModuleReferences` | 1095–1242 | 148 | whole blocks: `move_item` of the block lines already |
| `impl RustBackend` #2 (`resolve_opening`, `anchor_opening`, `outside_references_opening`) | 1243–1483 | 241 | **this op** |
| `impl RustBackend` #3 (`references_outside` … `rename_placeholder`) | 1485–2310 | 826 | **this op** |
| free items (`Produced`, `OutlineItem`, `LspEdit`, `PathReached`, …, `mod` wiring) | 2311–3008 | ~698 | free items (nodes 2, 3) |

**Finding: this op alone cannot bring `rust.rs` under 500.** Moving every inherent member except `new`
removes about 1,620 lines. The trait impls are another 148, and `move_item` can move those today. That
still leaves about 1,200 lines of free items (lines 1–515 and 2311–3008) for node 17's other parents
(`multi-seam-extract`, `tidy-facades`). Node 17 already depends on them. Its plan should be read as
"this op for the members, `move_item`/`extract_module` for the rest", and not as "via node 13" alone.

**The layout node 17 produces is already the house pattern.** Ten child modules of `backends::rust`
hold `impl RustBackend` blocks today: `item_move.rs:53`, `module_reparent.rs:43`, `retarget_impl.rs:77`,
`repoint_call.rs:48`, `repoint_facade.rs:45`, `signature.rs:81`, `imports.rs:18`, `documents.rs:23`,
`readiness.rs:42` and `item_path.rs:257`. Each imports the type with `use super::{…, RustBackend}`.

### E2.3 — The visibility rules for members, verified against the language

- A private **method or associated item** is private to the module that holds the `impl` block, not to
  the module of the type. Moving `fn start` from `backends::rust` to `backends::rust::transport` makes
  it invisible to `backends::rust` and to its other children. `start` is called from `item_move.rs`,
  `module_reparent.rs`, `retarget_impl.rs` and others (`self.start(workspace.root)?`,
  `item_move.rs:73`). The narrowest legal visibility there is `pub(super)`, which covers the
  `backends::rust` subtree.
- A private **field** is private to the struct's module. `struct RustBackend` is at `rust.rs:425` with
  private fields. A destination inside `backends::rust` still sees them. A destination outside it needs
  each field the moved code reads widened (E0616).
- A moved member that calls a private member which stays needs nothing when the destination is a
  descendant of the module holding that member's block. In node 17's case every destination is a child
  of `backends::rust`.
- **The bound on what must be surveyed** is the same as for node 1's reparent. A private member of `T`
  stays visible to the moved code only from the modules where it was visible before. Those are the
  origin and its ancestors. Of those, the only ones that can need widening are the ones strictly below
  the common ancestor of origin and destination. A destination that is a descendant of the origin
  needs none: no remaining member, no field and no root item has to be widened.
- **Callers need no re-pointing.** A method is reached through its receiver, `Self` or its type
  wherever its block is written. `retarget_impl` relies on the same fact in reverse (plan-schema
  "a method call (`self.m()`) … left as written"). The extract-module path proves it live
  (`impl_seam_acceptance.rs`). No `use` can bind an associated item, so there is no import to re-point
  and no facade to leave.

### E2.4 — Reusable pieces (and what is not reusable as is)

| Need | Piece | State |
|---|---|---|
| the run of members of **one** inherent `impl`, contiguous, none cut in half | `retarget_impl/outline.rs:55-120` `read` / `block` / `refuse_a_cut_member` (`Run`, `Member`, `whole_block`) | **reusable**, but `pub(super)` inside a private `mod outline;` (`retarget_impl.rs:12`), and its refusal text says "a retarget changes one block's self type". It needs a visibility change and the operation's name passed in |
| member anchors `c::m::Type::member`, `items` on members, `<Type>#N` | `item_anchor` (used by `retarget_impl`, plan-schema "Member run") | **reusable** as is |
| moved members' private visibility and that of the stayed members they call | node 1's member survey/widening (`item_move/members.rs`, per node 1's PRD "Shape: … `#reshape` 13 consumes it") | **consumed** from node 1 |
| root items of the origin the moved lines name, and `use` lines for them | `item_move.rs:192` `reached_by_the_moved_code`, `item_move/outline.rs:150` `left_behind`, `item_move/imports.rs:33` `needed` | **reusable** (the moved text is a byte range, so the readers apply unchanged) |
| widening a reached root item for a destination outside the origin | the second half of `item_move/assemble.rs:206` `visibilities` (71 lines, on node 19's list; `assemble.rs` is 507 lines and `#reshape` 15's target) | **not callable alone**: the reached-item half must be extracted with `extract_method` (engine) before reuse. Neither function may grow |
| relative paths/visibilities in moved code (`super::x`, `pub(super)`) | `item_move/rebase.rs:36-96` `edits` | **reusable** with `travelling: None` |
| where the block lands: above a trailing `#[cfg(test)]` module; inline module indentation | `item_move/placement.rs:15` `insertion` | **reusable** (it matters for node 17: lines after a test module are not counted as production, and they trip `clippy::items_after_test_module`) |
| creating the destination (`name`) | `item_move/creation.rs:36` `within`, `Destination` | **reusable** |
| destination module lookup, same-crate checks | `item_move/destination.rs`, `item_move/preflight.rs:85` | **reusable** |
| `verify` | `impl` headers are not statements to `verify` (`tests/verify_accounts_for_a_retarget.rs:26-28`), and a leading `pub…` is stripped (`src/verify.rs:21-24`) | expected to hold **without** a new rule. A test pins it |

### E2.5 — Wiring cost in files on the size lists

- `rust.rs` `SUPPORTED` (`:75`, 25 kinds) gains one entry. `LanguageBackend::check` (`:1124`, 66 lines,
  node 19's list) and `resolve_opening` (`:1245`, 163 lines, node 19's list) each dispatch the same-crate
  moves with one `if` arm per kind (`:1125-1133`, `:1291-1305`). A new arm grows both, which the binding
  decision forbids. **Option:** fold `MoveItem`, `ReparentModule` and the new kind into one arm per
  function, calling a dispatcher in a new file. Both functions then shrink by about 3 lines.
- `plan/codec.rs` (480 lines) dispatches per-op rules to `plan/codec/*_fields.rs`. The new rules go in a
  new `plan/codec/impl_move_fields.rs`, and `codec.rs` gains one `mod` and one call.
  `plan/refactor_kind.rs` (203) gains a variant with its doc. The `reexport` rule (`codec.rs:302-314`)
  already refuses `reexport` on any kind it does not list, so it needs no edit.

### E2.6 — Hazards the op must answer (found by reading, not reproduced)

- **A name the destination binds differently.** `imports::needed` leaves out a name in `taken`, the
  names the destination already binds. If the destination declares its own `fn relative_to` and the
  moved code called the origin's `relative_to`, the moved code silently binds the destination's. It
  compiles if the signatures agree. `move_item` has the same exposure. This op refuses that case.
- **`macro_rules!` scope is textual.** A member that uses a macro defined earlier in the origin file
  loses it in another file. `rust.rs` defines none (`grep macro_rules src/backends/rust.rs`: none).
  Left to the compile gate and recorded as a limit.
- **An origin block emptied by the run.** It is "unverified" in leftovers § 1. Node 17's runs keep `new`,
  so they never empty block #1, but a run over all of #2 or #3 does.
- **Attributes and generics on the block.** `impl<'a> Produced<'a>` exists (`rust.rs:2336`ff). The header
  has to be copied verbatim, and an existing destination block may be joined only if its header and
  attributes are identical.
- **Inline tests calling a moved private associated function** (`RustBackend::rewrite_signature` from
  `rust.rs`'s `mod tests`). `backends::rust::tests` is a sibling of the destination, so `pub(super)`
  covers it, and node 1's survey reads references under `cfg(test)` code too.

### E2.7 — Test harness

- Live suites over a one-package fixture: `tests/same_crate/mod.rs:32` `an_app_holding`, `:81`
  `a_move_item_op`, `:91` `a_move_item_into_a_new_module_op`, `:281` `the_impl_blocks_of` (already
  reads a file's `impl` blocks and their members, so it serves as the oracle for "which block holds
  which member"). Oracles: `harness::assert_compiles`, `assert_compiles_with_its_tests`,
  `assert_lints_clean`.
- `retarget_impl_acceptance.rs` and `retarget_impl_plan_lines.rs` are the model for a member-run op
  (fixture `a_host_impl_of`, `:57`). The live binary is registered in `.config/rust-e2e.filterset:72`
  and `.config/nextest.toml:102`. This node registers its own new suite only; node 1 registers the
  existing unregistered ones.
- Library-level tests go in the new assembly module ("a function of texts", like `item_move::assemble`),
  and the codec rules are tested in `src/plan.rs`'s plan tests, the way `retarget_fields` is.

# Initial discovery — #reshape 11/19 `move-item-paths`

Companion to [the changeset](2026-10-09-reshape-move-item-paths.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — move-item-paths (how same-crate moves and `retarget_impl` read a `use`)

Read on `pleasant-tent-b7473e01` @ `4a5c42b1b` (2026-10-09). Paths are relative to
`packages/tddy-code-restructuring/` unless they start with `packages/`, `docs/` or `.config/`.

### E2.1 One root cause behind the 10-08 facade miswrite: an extern-crate `use` is read as a relative path

The engine has one reading of "the module path a `use` names", `item_move::preflight::resolved_from`
(`src/backends/rust/item_move/preflight.rs:235-250`):

```rust
let mut path = match segments.first().map(String::as_str) {
    Some("crate") => Vec::new(),
    _ => at.to_vec(),            // every other head is read as relative to the module
};
```

A head that is not `crate`/`self`/`super` is pushed onto the importing module's path. Under Rust 2018
uniform paths, such a head is a **local** name only when the module binds it (a child `mod`, an item it
defines, a name it imports); otherwise it is an **extern crate**. `resolved_from` never makes that
distinction.

The chain that wrote `super::tddy_session_split::service_util::…` in `#carve` 21/21:

1. `packages/tddy-session-lifecycle/src/connection_service.rs:224-229` holds
   `pub use tddy_session_split::{attached_initial_prompt, service_util, …};` — a facade of another crate.
   `items_of_module` reads its leaf as `segments = ["tddy_session_split", "service_util"]`, no alias
   (`src/crate_move/source_scan.rs:245-264`; `UseLeaf` carries no visibility).
2. The moved block wrote `super::service_util::find_registered_project(…)` in module
   `connection_service::session_worktree_observer`. `rebase::path_edit`
   (`src/backends/rust/item_move/rebase.rs:99-158`) climbs `super::` to `arrives_at = [connection_service]`
   and asks the `imports` lookup about `service_util` (`rebase.rs:149-155`).
3. The lookup is `bindings::import_target` (`src/backends/rust/item_move/bindings.rs:16-37`), wired by
   `item_move/assemble.rs:356-358` and `module_reparent/assemble.rs:192-199`. `connection_service` does not
   define `service_util` and binds it by a plain `use`, so it returns
   `resolved_from(["tddy_session_split","service_util"], [connection_service])`
   = `[connection_service, tddy_session_split, service_util]` — a module that does not exist.
4. `path_edit` keeps the target's module part (`[connection_service, tddy_session_split]`) and
   `relative_to` spells it from the new module `connection_service::svc_session_identity_wiring`:
   `super::tddy_session_split::` (`rebase.rs:156`, `relative_to` at `rebase.rs:176-190`). `E0433`.

The path written before the move, `super::service_util::`, already meant the right thing at the
destination: the destination is a child of `connection_service`, and a module's imports — private or
public — are visible to every module below it. `path_edit` follows the import **unconditionally**: it
never asks whether the path to the importing module still works from the destination, nor whether the
import is `pub` (a facade written to be named). Today it also rewrites a `super::Config` that needs no
rewrite whenever the importing module is an ancestor of the destination (same-crate import followed to
the definition, `super::super::types::Config`), which is noise but compiles.

The correct 2018 rule already exists once, in `crate_move::reexports::Walk::absolute`
(`src/crate_move/reexports.rs:171-198`): "`crate`, `self` and `super` are resolved against the module's own
path; a first segment that names a child module is that module …; anything else already names a crate."
It only checks child modules (not items a module defines or names it imports), and it is a private method
of a cross-crate walk over extern-name-rooted strings, so it cannot be called from `item_move` as is.

`resolved_from` has two more callers, both in the `move_item` clash check
(`preflight.rs:188`, `preflight.rs:223`). There the misread is harmless — an extern path never equals the
moved item's path, so the binding still counts as a clash — but it is right for the wrong reason.
`item_path.rs:285,315` call a different, local `resolved_from`.

### E2.2 `retarget_impl` S6 compares `use` paths as text

`retarget_impl/imports.rs:36-57`:

- builds `path = ["crate", <to_type module>…, Name]` (`:36-39`);
- reads the file's top-level `use` leaves with `items_of_module` and treats the name as already bound only
  when a leaf binding `Name` has `segments == path` **verbatim** (`:41-45`);
- otherwise refuses when `names_declared_in(&text)` contains the name (`:50-57`) — and
  `names_declared_in` (`item_move/preflight.rs:273-284`) counts every `use`-bound name, so
  `use super::launch_ports::LaunchSessions;` (the same item) is reported as "bound to something else"
  (`E0255`), exactly the 2026-10-07 entry.

Spellings of the same item that S6 refuses today: `super::m::T`, `self::m::T`, a 2018 child-relative
`m::T`, and any path with a different number of `super::` steps that lands on the same module. The
module path S6 needs is at hand: `the_use` already calls `module_path_of(workspace.root, file)`
(`:32`, `src/item_anchor.rs:46-…`), which returns `[crate_name, modules…]`; `[1..]` is the path below the
crate root that `resolved_from` takes. S6 runs only after the server has started (`retarget_impl.rs:134`,
after `self.start` at `:90`), so a static `check` never reports it — unchanged by this node.

The whole-work discovery's pointer ("reuse `import_text.rs`") is not the right helper:
`import_text::imported_paths` (`src/backends/rust/import_text.rs:236-246`) expands `use` text into strings
and resolves nothing. The reusable reading is `resolved_from` (fixed per E2.1).

The hand fix it caused still carries a marker that points at the todo this node deletes:
`packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs:15-17`
(`// TODO(restructure-retarget-impl-s6): spelled crate:: by hand …` over
`use crate::launch_ports::LaunchSessions;`). The block has since been retargeted (`impl LaunchSessions`
at `:19`), so only the comment is stale.

### E2.3 `super::Name` through an aliased or a glob import is not followed (`bindings.rs:32-35`)

`import_target` finds the importing module's binding with
`.find(|leaf| leaf.alias.is_none() && leaf.bound_name() == Some(name))` (`bindings.rs:32-35`):

- **Alias** — `use crate::types::Config as Settings;` and moved code writing `super::Settings`: the leaf
  is skipped (`alias.is_some()`), the lookup returns `None`, `path_edit` respells the path to reach the
  importing module, and a destination outside it gets `super::host::Settings` — a private import named
  from outside its module (`E0603`). Even if the leaf were found, `path_edit` would drop it: it keeps the
  target only when its last segment equals the name written (`rebase.rs:153-154`), and an alias's target
  ends in `Config`, not `Settings`. Following an alias means rewriting the **name token** too, which
  `path_edit` never does (its edit is `at..cursor`, the prefix).
- **Glob** — `use crate::types::*;`: `bound_name()` is `None` for a glob (`source_scan.rs:255-258`), so
  nothing matches and the result is the same `E0603` for a private glob. A glob import has to be
  *confirmed* — its module must bind the name — before it counts, which is how
  `crate_move::reexports::Walk::walk` already treats globs (`reexports.rs:131-137`).

### E2.4 The "aliased import reads as a name clash" limit is misdescribed; the real alias defect is in `sites.rs`

The 2026-10-05 entry and the feature doc (`docs/ft/coder/rust-code-restructuring.md:983-985`) say a
destination that imports the moving item under an alias "is refused as a clash". Per the code it is not:
the clash check (`preflight.rs:168-200`, `taken_by_something_else`) starts from `names_declared_in`, which
holds the **alias** (`bound_name()` returns the alias, `source_scan.rs:259-262`), not the moved name, so a
destination with `use crate::pairing::peer as check;` does not report `peer` as taken.

What does go wrong is later: `sites.rs` re-points every `use` of a moved item, and in the destination
itself (`in_destination`, `src/backends/rust/item_move/sites.rs:109-115,143`) it **drops** the statement
(`rewrite_statement`, `sites.rs:354-358` for a plain `use`, `sites.rs:381-387` for a group member) — with
its alias. The destination then names `check` with nothing binding it (`E0425`/`E0412`), and the compile
gate stops the run. Dropping is right only for an un-renamed import (the destination now defines the
name). A unit test already pins the alias being kept for a non-destination caller
(`sites.rs:447-455`, `keeps_an_alias_and_a_visibility_of_a_plain_use`); nothing pins the destination case.

The glob half of the clash check is already handled: `reexports` (`preflight.rs:204-232`) accepts a
destination import that goes through a glob of the source module, pinned by
`moves_an_item_into_the_module_that_imports_it_through_a_glob_reexport`
(`tests/move_item_of_an_item_the_destination_imports_acceptance.rs:63-107`).

Related, not claimed: `reparent_module`'s clash check `name_taken`
(`src/backends/rust/module_reparent/survey.rs:186-195`) uses `names_declared_in` with **no** exemption for
an import of the very module that moves, unlike `move_item`'s `taken_by_something_else`.

### E2.5 What the rest of the claimed entries name, checked

- 10-08 miswrite, tidy-skipped half (`unused_imports` left because the compile gate failed first): node 3's
  slice (`runner/tidy*`), untouched here.
- 2026-10-05 same-crate limits: the emptied directories and the `pub use`-chain widening are node 1's;
  import placement after nothing in an empty created file (cosmetic) and "one new module per `name`" are
  not claimed by any node and stay in the entry.

### E2.6 Reusable pieces

| Need | Existing helper |
|---|---|
| A module's top-level `mod`s, defined names and `use` leaves (segments, alias, glob) | `crate_move::source_scan::items_of_module` (`src/crate_move/source_scan/module_items.rs:32-95`) |
| The file and scope of a module path inside a package | `item_move::destination::find_module` / `Lookup` (used by `bindings.rs:22`) |
| A visibility keyword as the module subtree it covers | `item_move::scope::Scope::parse` (`src/backends/rust/item_move/scope.rs:23-50`) |
| A relative prefix from one module to another | `rebase::relative_to` (`rebase.rs:176-190`) |
| A file's module path | `item_anchor::module_path_of` (`src/item_anchor.rs:46`) |
| 2018 head rule (child module vs crate) | `crate_move::reexports::Walk::absolute` (`reexports.rs:171-198`) — private, string-based; the rule is restated, not called |

`UseLeaf` has no visibility (`source_scan.rs:245-250`); a `pub use` and a private `use` read the same.
`ChildModule` already reads its own `is_public` the same lexical way (`module_items.rs:17-19,67-68`), so
recording a `use`'s visibility text is a local change to the scanner.

### E2.7 Test harness and fixtures

- `tests/same_crate/mod.rs`: `an_app_holding` (one `app` package), `an_app_over_a_kernel`
  (`app` over a path crate `kernel` — exactly the extern-facade shape of the 10-08 run, `:190-230`),
  `moving_items`, `moving_items_into_a_new_module`, `reparenting_module`, `what_a_static_check_finds_in`,
  `the_anchor_over`.
- Oracles in `tests/harness/mod.rs`: `assert_compiles` (`:908`), `assert_compiles_with_its_tests`
  (`:916`), `assert_lints_clean` (`:2673`), `applying_a_plan_of` (`:2452`), `checking_the_plan` (`:2494`).
- Unit level, no server: `rebase.rs` tests drive `edits` with an `imports` closure
  (`rebase.rs:259-283`, `follows_an_import_of_the_module_a_super_path_names`); `preflight.rs:297-303` pins
  `resolved_from`; `sites.rs:423-455` pins `rewrite_statement`; in-crate workspace tests over a temp dir +
  `Overlay` exist in `crate_move/reexports.rs:263-317`.
- `retarget_impl`'s S6 sits behind a live server (`retarget_impl.rs:90` before `:134`); its suite is
  `tests/retarget_impl_acceptance.rs` (live, registered in `.config/rust-e2e.filterset:72` and the
  `rust-analyzer` group, `.config/nextest.toml:102`). Its helper `the_anchor_over_members_of_host` hardcodes
  `app::host::` / `src/host.rs` (`:63-73`), so a fixture with the type in a nested module needs a helper
  of its own. The S6 clash refusal for a *different* item is pinned by
  `refuses_a_use_that_would_clash_with_a_type_the_file_already_binds` (`:522-554`) and must stay green.
- **Registration gap (pre-existing):** eleven live-rust-analyzer suites say "enforced … by the
  `rust-analyzer` test group" but are in neither `.config/rust-e2e.filterset` nor the group:
  `move_item_acceptance`, `move_item_beyond_the_basics_acceptance`, `move_item_creates_module_acceptance`,
  `move_item_into_an_existing_module_acceptance`, `move_item_of_an_item_the_destination_imports_acceptance`,
  `move_item_outside_facade_acceptance`, `reparent_module_acceptance`,
  `reparent_module_beyond_the_basics_acceptance`, `reparent_module_through_the_old_parents_import_acceptance`,
  `same_crate_deep_check_acceptance`, `signature_rewrites_acceptance`. New live tests of this node go in a
  new binary that **is** registered, rather than into those suites.

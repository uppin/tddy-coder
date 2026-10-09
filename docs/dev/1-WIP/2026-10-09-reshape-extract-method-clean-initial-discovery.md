# Initial discovery — #reshape 4/19 `extract-method-clean`

Companion to [the changeset](2026-10-09-reshape-extract-method-clean.md). Exploration 1 is the stack's whole-work discovery, copied in full; Exploration 2 is this node's own.

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

## Exploration 2 — extract-method-clean (what `extract_method` writes, re-measured against the dev shell's rust-analyzer)

**Date:** 2026-10-09. Paths relative to `packages/tddy-code-restructuring/src/` unless noted. Every claim of the
two claimed entries (`2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md`, items
P, Q, R, S, T, U, V; `2026-09-24-restructure-apply-leaves-the-lint-gate-red.md`, N3) and of the two partial items of
`2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md` (K, I) was checked against the current code
and, where the claim is about what rust-analyzer writes, **reproduced against the rust-analyzer the dev shell ships**
(`/nix/store/yab4lviy5v50kbykq4rlrz6irs5p98r6-rust-analyzer-2026-03-30`, the one `./dev` puts on `PATH` and the engine
spawns). The reproduction is a 90-line stdio LSP driver over a scratch crate (`initialize` → `didOpen` → wait for
`experimental/serverStatus` quiescent → `textDocument/codeAction` with `only: ["refactor.extract"]` →
`codeAction/resolve`), kept outside the repository. Its findings change three of the entries' claims; see
"Corrections to the backlog" at the end.

### 1. Where an `extract_method` goes today

| Step | Where | What |
|---|---|---|
| Assist table | `backends/rust.rs:291-301` | `ExtractMethod` → title `extract into function`, placeholder `fn` **with a fixed name `fun_name`**, `needs_inference: true`, `relocates_items: false` |
| Text-only refusals (`check`) | `backends/rust.rs:1154-1166` | `refuse_early_returns`; the function-local `use` carry is a progress line, not a finding |
| Text-only refusals (`resolve`) | `backends/rust.rs:1262-1272` | the same refusal, before a server starts |
| Inference wait | `backends/rust.rs:1545-1548` | probe = `selection::hover_bearing_position(original, range)`, then `wait_until_resolved_within_bound` |
| Assist | `backends/rust.rs:2254-2271` (`extract`) | the same probe is passed as the assist's position |
| Naming | `backends/rust.rs:1581-1594` | `introduced_by` (fixed placeholder) → server rename → `refuse_residual_placeholder`, `refuse_inferred_placeholder` |
| **Post-processing** | `backends/rust.rs:1596-1604` | the `!relocates` branch: **only** `imports::carry_function_local_uses`, then return. No import pass, no comment check, no signature clean-up |
| Edit | `backends/rust.rs:1996-2018` (`edit_for`) | `minimal_edits(original, text)` — one `FileEdit::Change` |

Everything `extract_module` gets after its assist (`prune_assist_imports`, `restore_imports`, `carry_shadowed_imports`,
`restore_visibility`, `rerooted_module`, `rust.rs:1606-1638`) is skipped for `extract_method`. That is gap **K**'s
cause, confirmed: "the import pass never runs for `extract_method`".

### 2. What rust-analyzer 2026-03-30 writes (reproduced)

Fixture ranges and the assist's output, verbatim (`fun_name` is the server's placeholder, before the engine's rename).

**P — comments are dropped only when the range holds a `?`.** A range of plain statements keeps every comment:

```rust
// range: four statements and two full-line comments, no `?`
fn fun_name(dir: &PathBuf, label: &String, items: &Vec<u32>) -> (usize, usize, PathBuf) {
    let mut started = items.len();
    // why: the records above carry the rest
    let doubled = started * 2;
    // the readiness gate
    let joined = dir.join(label);
    (started, doubled, joined)
}
```

A range holding a `?` (here `self.seeded(agents).await?`, the 09b shape; and a sync `Err(..)?` in an `if`) loses every
comment **between statements** and every **trailing** comment, and keeps a comment **inside** a statement (in a method
chain):

```rust
// before (range = the first four statements of `start`)
        let mut started = self.seeded(agents).await?;
        // The defs behind those records, which the jail env can only carry for agents this host
        // holds — the records above are what carries the rest.
        let defs = self
            .roster
            .iter()
            // only the held ones
            .filter(|r| agents.contains(r))
            .count();
        let p = dir.join("x"); // trailing note

// after
    async fn fun_name(&self, agents: &[String], dir: PathBuf) -> Result<(Vec<String>, usize, PathBuf), String> {
        let mut started = self.seeded(agents).await?;
        let defs = self
            .roster
            .iter()
            // only the held ones
            .filter(|r| agents.contains(r))
            .count();
        let p = dir.join("x");
        Ok((started, defs, p))
    }
```

So the assist rebuilds the statement list (to wrap the outputs in `Ok(…)`) from statement nodes and drops the trivia
between them. The entry's "suspected cause" is right, and narrower than it reads: the 09b/10b/09c ranges all held `?`
or `.await?`. The engine has nothing today that sees the loss — `verify` reports it afterwards (`verify.rs:7-41`, test
`reports_an_orphaned_comment_that_was_dropped` at `verify.rs:275`), and `apply` does not run `verify`.

**Q — the four mechanical shapes, all reproduced.**

| Shape | Reproduced as | Lint |
|---|---|---|
| a `PathBuf`/`String`/`Vec<T>` local borrowed by its own type | `fn fun_name(dir: &PathBuf, label: &String, items: &Vec<u32>)` | `clippy::ptr_arg` |
| `let mut` copied into the callee | callee `let mut started = …; … (started, doubled, joined)` with no mutation in the callee; the caller binds `let (mut started, doubled, joined) = fun_name(…)` and mutates | rustc `unused_mut` (callee) |
| a unit-typed range holding a `?` becomes the tail inside `Ok` | `fn fun_name(dir: &PathBuf, flag: bool, n: usize) -> Result<(), String> { Ok(if flag { … Err::<(), String>("e".into())?; }) }` | `clippy::unit_arg` |
| `field: &field` rewritten to the parameter | `consume(&Lookup { base: base, root: root })` in `fn fun_name(base: &PathBuf, root: &PathBuf)` | `clippy::redundant_field_names` |

The 10b reverse case (the **caller's** `mut` left unused once the callee takes `&mut`) is the same `unused_mut` lint at
another place. Arity (`too_many_arguments`) and tuple returns (`type_complexity`) are the plan's, as the entry says.

**rust-analyzer reports none of these.** After applying the assist's output, `textDocument/diagnostic` returned an empty
list — also with `initializationOptions.diagnostics.experimental.enable = true` — so the server has no `unused-mut`
signal to read. It **does** report a type mismatch: with `&PathBuf` → `&Path`, `&String` → `&str`, `&Vec<u32>` →
`&[u32]` rewritten by hand and a body that passes the parameter to `fn needs_buf(_: &PathBuf)` and binds
`let c: PathBuf = dir.clone()`, the pull diagnostics are `E0308 expected &PathBuf, found &Path` and
`E0308 expected PathBuf, found &Path` at that line. That is a usable oracle for a `ptr_arg` rewrite; the call sites need
none (`&PathBuf` coerces to `&Path` at a call).

**K — reproduced.** The origin spells the trait qualified (`Option<std::sync::Arc<dyn deep::recipe::WorkflowRecipe>>`),
and nothing imports it. The assist writes `fn fun_name(r: Option<std::sync::Arc<dyn WorkflowRecipe + 'static>>) -> String`
— a short name that resolves nowhere in the file (`E0405`). Its `Arc` stays qualified, so the server prints a path it
can see for one type and not the other.

**R — the probe position, not the assist.** For a range starting on a bare block's `{` and for one starting on a line
comment, `textDocument/hover` at the range's start is `null` on a quiescent server, and the assist itself is offered and
resolves correctly for both (the block one even drops the braces: `fn fun_name(y: &mut u32) { let z = *y + 1; *y = z * 2; }`;
the comment-first one keeps the comment). So R is the probe, which is what `hover_bearing_position` exists to choose:
`selection.rs:15-46` skips `&`, `&mut`, `*`, `!`, `-`, `(` and whitespace **on the first line only**, so a range opening
on `{` or `//` probes exactly there.

**S — explained.** For a range that is the initializer of a `let` (`let managed = if flag { … } else { … };`, the
`if … else` selected), the assist names the function after the binding — `fn managed(flag: bool, dir: &PathBuf) ->
Option<PathBuf>` and `let managed = managed(flag, &dir);` — not `fun_name`. The engine looks only for
`fn fun_name` (`introduced.rs:41-52`, fixed name from `rust.rs:297`), so it refuses with "rust-analyzer did not produce a
`fn fun_name` to name" (`introduced.rs:49`), the exact 10b refusal. `introduced.rs` already finds a server-named symbol
by what the assist added, but only for `let` (`introduced_binding`, `introduced.rs:70-104`; `bindings` scans `let` only,
`introduced.rs:107-110`).

### 2b. A range holding `return` (the guard lift added by the developer on 2026-10-09)

`early_return::refuse_early_returns` (`backends/rust/early_return.rs:34-60`) refuses a range with any `return` unless it
runs to the function's tail and the function is not `()`. `early_returns` (`:140`) is the lexical scan (closures, `async`
blocks, nested `fn`s excluded); `runs_to_the_end_of_a_function` (`:172`) is the exception. What the server does with the
ranges the refusal stops, reproduced:

```rust
// a mid-function run of `return Err(..)` guards (an `if` and a `for` holding one), in `fn guards(a: u32, b: &str) -> Res<u32>`
    let n = a + 1;
    if let Some(value) = fun_name(b, n) {
        return value;
    }
    Ok(n * 2)
fn fun_name(b: &str, n: u32) -> Option<Result<u32, String>> {
    if n > 10 {
        return Some(Err(format!("too big {n}").into()));
    }
    for c in b.chars() {
        if c == 'x' {
            return Some(Err("x".into()));
        }
    }
    None
}
```

It is well-typed (the `.into()` target is inferred from the return type), and it is exactly one fixed shape, so a
deterministic rewrite into `fn …(…) -> Res<()>` + `fun_name(b, n)?;` is sound. The return type is printed with the
alias expanded (`Res<u32>` became `Result<u32, String>`). With this crate's `use crate::{RestructureError, Result};`
(a one-argument alias), the expanded `Result<X, RestructureError>` would be `E0107`, so the spelling has to come from
the caller.

```rust
// the body of a match arm in tail position (`refreshed`'s `Item` arm shape: `let … else { return Ok(base); }` and `return Err(..)`)
        _ => {
            fun_name(k, v)
        }
fn fun_name(k: u8, v: Option<u32>) -> Result<u32, String> {
    let base = k as u32;
    let Some(found) = v else {
        return Ok(base);
    };
    if found > 9 {
        return Err("big".into());
    }
    Ok(base + found)
}
```

The server keeps the `return`s verbatim and makes the call the arm's value. This is correct because the arm is the
function's result, the same reasoning as today's runs-to-the-tail exception.

A guard run that **also propagates with `?`** (`check_len(b)?; if n > 10 { return Err("big".into()); }
check_len(&b[1..])?;`): the server writes the lifted form itself — `fn fun_name(b: &str, n: u32) -> Result<(), String>`,
the `return Err` verbatim, tail `Ok(())`, and the call `fun_name(b, n)?;`. That is a second accepted shape, and it needs no
rewrite beyond the return type.

**The return type of a plain `?` range (no `return`)**, which `#reshape` 16 hit:
- with the alias **imported** (`use crate::aliased::Result;` where `type Result<T> = std::result::Result<T, String>;`), the
  server writes `fn fun_name(b: &str, x: u32) -> Result<(u32, u32), String>`. The `Result` in scope takes one argument, so
  this is `E0107`;
- with the alias **defined in the same module**, it writes `std::result::Result<u32, String>`, which compiles.

So the caller's spelling has to be applied to **every** extraction, not only to guard lifts.

Targets: `plan/codec.rs::parse_op` (`:246`, 206 lines) is mostly `if … { return Err(malformed(…)); }` guards
(`:287-475`), plus one inside `for field in CODE_BEARING_FIELDS` (`:254-259`). `plan_store/refresh.rs::refreshed`'s `Item`
arm (`:70-101`) returns `Ok(anchor)` through two `let … else`, so it is a **value** return: only the tail-position
extension reaches it, not an `Err` guard lift. The existing pin `refuses_a_range_that_returns_early_from_the_enclosing_function`
(`tests/extract_method_control_flow_acceptance.rs:33`) uses a value return (`return Ok(1);`) and stays refused.

### 3. Today's state of the readiness half of R

#542 (`#live-plan` 5/7, after the entry was written) bounded the probe: `READY_HOVER_BOUND` = 30 s
(`backends/rust/readiness.rs:26`), applied in `await_answer` only while `self.indexed && !self.chatter.loading()`
(`readiness.rs:174,187`). So, today:

- on a backend whose warm-up ran (`indexed == true`), a range opening on `{` or `//` is **refused after 30 s** with
  "rust-analyzer still gives no hover at … Start the range on an expression it can type" — wrong advice for a range the
  assist handles;
- on a backend whose warm-up was skipped (`readiness.rs:68-70`, "no indexable symbols in file", the apply-gaps item W
  path), `indexed` stays false and the same wait has **no bound** — the original hang. That generic deadline is node 10's
  (`apply-robust`); this node removes the cause for these two shapes by probing somewhere that can be typed.

### 4. I and N3 — the trait import nobody carries

- `prelude_shadow::shadowed_imports` (`backends/rust/prelude_shadow.rs:47-62`) carries a parent binding into the new
  module when the moved code **names** it. `names_bound` reads `use a::Trait as _;` as binding nothing
  (`imports/bound_names.rs:8`), so an anonymous trait import is never carried; and a trait used only for method resolution
  is never named, so a named trait import used that way is not carried either.
- The server-driven pass keys on `unresolvedReference` semantic tokens (`backends/rust.rs:1900`, `unresolved_names`); an
  unresolved *method* is not one, so it never sees `encode_to_vec`/`decode`.
- N3's open half is the same defect: the lint-gate entry's own example parent carries `use prost::Message as _;`
  (its N1 listing), and the seam that calls `encode_to_vec` got nothing. Its other half (the unused copy) is closed by the
  tidy (`runner/tidy.rs:412-417`, `unused_imports` only).
- The existing design for exactly this class is "over-import, let the tidy remove what rustc reports unused"
  (`prelude_shadow.rs:1-10`; `docs/ft/coder/rust-code-restructuring.md` § Import restoration). The tidy's removal is
  trait-aware because rustc's is.

### 5. U — a server panic is classed as a malformed plan

`backends/lsp_bridge.rs:136-153` (`map_lsp_error`): `ContentModified` and `Timeout` → `ServerCatchingUp`, `Abandoned` →
`CallerStopped`, **everything else** → `MalformedPlan("lsp: …")`. A `-32603 request handler panicked` is the server's
defect (`RestructureError::ServerDefect`, `lib.rs:63`, "rust-analyzer's answer was unusable"), and the refusal-class table
(`docs/ft/coder/rust-code-restructuring.md` § Refusal classes) says a malformed plan is for the author to fix.

### 6. A compiler-guided import repair (the brief's suggestion for K / I / N3) — what it would need

- **Where:** `runner/compile_gate.rs:123-126` — between a failing result check and `AppliedTreeDoesNotCompile`; the tidy
  runs only when the tree compiles (`compile_gate.rs:146-178`).
- **Evidence it lacks:** "restore only bindings the original parent declared" needs the files' pre-run text. The journal
  keeps **hashes**, not text (`journal.rs:66-71`); only a transactional group keeps pre-images (`journal.rs:95-97`).
- **Suggestions it would read:** rustc's "consider importing" and "trait … is implemented but not in scope" suggestions are
  `MaybeIncorrect`; the tidy's parser keeps only `MachineApplicable` fixes (`runner/tidy/diagnostics.rs:121-133`).
- **Parity:** it would run in `apply` only; `check --deep` would still report a plan clean that the gate then repairs, or
  fails.
- **Collisions:** `compile_gate.rs` is node 10's (destination packages in the gate), `tidy*.rs` is node 3's.

### 7. The tidy and `unused_mut`

`runner/tidy.rs:251-300` (`tidy_imports`) applies only `unused_imports` removals; every other warning in a touched file is
printed as `warning remains: …` (`tidy.rs:526-540`). rustc's `unused_mut` carries a `MachineApplicable` "remove this `mut`"
suggestion, which `diagnostics.rs:121-133` already parses. `apply_tidy_acceptance.rs` drives the tidy through a test-binary
move with no rust-analyzer (`tests/apply_tidy_acceptance.rs:19-44`), so an `unused_mut` case can use the same harness.

### 8. Test harness to reuse

- Live rust-analyzer, one server at a time: `tests/harness/mod.rs` — `performing` (`:381`, resolve + apply, panics on a
  refusal), `resolving` (`:410`, returns the refusal), `an_extract_method_of(fixture, file, lines, name)` (`:1756`),
  `a_workspace_holding_files` (`:2643`) + the `one_crate_holding` pattern (`tests/extraction_defects_acceptance.rs:21-33`),
  `assert_compiles` (`:908`), `cargo_check_all_targets` (`:181`), `is_rustfmt_clean` (`:202`). There is **no** clippy
  helper; nothing in `src/` or the harness runs clippy.
- Registration of a new live binary: `.config/rust-e2e.filterset` (`extraction_defects_acceptance` at `:51`,
  `extract_method_signature_acceptance` at `:49`) and the `rust-analyzer` group in `.config/nextest.toml` (`:94`).
- Library-level, no server: unit tests beside each pure text function (the `selection.rs`, `early_return.rs`,
  `imports/local_uses.rs` pattern), `lsp_bridge.rs`'s own `mod tests` (`:156`) for U.

### 9. Sizes this node must not grow

Production lines (to the first `#[cfg(test)]`): `backends/rust.rs` ≈2,983 (node 17 splits it — this node adds wiring
only), `backends/rust/early_return.rs` 740 total / 531 production, `backends/rust/imports.rs` 524 total (≥500 — no new code
there), `backends/rust/selection.rs` 224, `backends/rust/introduced.rs` 186, `backends/rust/prelude_shadow.rs` 191,
`runner/tidy.rs` 1,034 total. New logic goes into new modules under `backends/rust/`.

### Corrections to the backlog (for the changeset's Prerequisites)

1. **P is shape-specific:** comments survive unless the range holds `?`; then inter-statement and trailing comments go,
   in-statement ones stay. The fix can therefore be aligned statement-by-statement (the assist keeps the statements in
   order and appends one tail).
2. **R is no longer a hang on a ready backend** — it is a 30 s refusal with wrong advice since #542; it is still unbounded
   when the warm-up was skipped (node 10's deadline). The assist handles both shapes; only the probe is wrong.
3. **S is not a range defect:** the server named the function after the `let` it initialises.
4. **T is fixed** (`placeholder_checks.rs`, test `names_a_parameter_whose_type_borrows_through_an_elided_lifetime`,
   `tests/extract_method_signature_acceptance.rs:36`). **V** is the index daemon's (it does not watch the tree), not this
   crate's. The three 10b build breaks (`*log::warn!` E0614, E0505 by-move-while-borrowed, E0308 by-value) are
   rust-analyzer rewrites the compile gate reports; none was reproduced here.
5. **The lint-gate entry is down to N3's missing half**, which is gap I.

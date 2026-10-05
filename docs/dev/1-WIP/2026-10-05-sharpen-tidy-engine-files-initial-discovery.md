# Initial Discovery: `tidy-engine-files` (`#sharpen` 1/8) — three engine files under the 500-line budget

**Changeset**: [2026-10-05-sharpen-tidy-engine-files.md](./2026-10-05-sharpen-tidy-engine-files.md)
**Date**: 2026-10-05
**Passes**: 2 (Exploration 1 is the whole-work discovery of the `#sharpen` stack, copied in full; Exploration 2 is this node's own pass)
**Tree**: worktree `.worktrees/engine-fixes-plan`, `origin/master` `a77bca29`. Read-only analysis.

## Combined Conclusions

Narrowed to this node. The stack-wide conclusions (what the other seven nodes do, the D1/D2 questions, the
shared-resolver edge) are in Exploration 1 and are not repeated here.

1. **Measured, on master `a77bca29`, by the rule `check --budget` uses** (`runner/budget.rs:production_lines`:
   lines before the first `#[cfg(test)]` whose next non-blank line opens a `mod`): `src/plan.rs` **520**
   (its test module opens at line 521), `src/item_anchor.rs` **517** (test module at 518),
   `src/plan/codec.rs` **514** (no test module: the whole file, `wc -l` 514). Those are the three numbers
   in the todo, and they hold. The only other files of `src/` over 480 production lines are
   `backends/rust.rs` (2,852) and `crate_move/test_binary.rs` (967), both of which have records of their own
   and are not this node's, and `backends/rust/item_path.rs` at 490 (inside the budget, ℹ close).
2. **The seams the todo names exist and are contiguous**, so each is one anchor:
   - `item_anchor.rs` lines 76-137: `owning_package` (`pub(crate)`) + `repo_root_hint` + `collect_package_files`
     (the last two private, called only from `owning_package`) = 62 lines + a blank. One outside caller
     (`item_move/destination.rs:12,26`) and one inside (`module_path_of`, line 45).
   - `plan/codec.rs` lines 212-254: `hint_of` (`pub(crate)`) + `rfc3339` (`pub(crate)`) = 42 lines + a blank;
     and lines 271-293: `refuse_split_groups` (private, one caller: `parse_ops`) = 23 lines + a blank.
     `hint_of` is reached from `plan.rs:502` (`pub(crate) use codec::hint_of;`) and, through that, from
     `plan_store/live.rs:19,144`; `rfc3339` is reached only from `plan.rs`'s `mod tests` (`:524, :1522, :1529`).
   - `plan.rs` lines 136-325: `RefactorKind` (the enum, 138 lines of mostly docs) + `impl RefactorKind`
     (two predicates, `edits_a_call_site`, `moves_across_crates`) = 189 lines + a blank; or, the todo's
     narrower reading, `Reexport` + `impl Reexport` at lines 326-358 (33 lines).
3. **The numbers the seams buy** (predicted from those line ranges plus two or three lines of `mod` and
   `pub use` each; the measurement is the gate, not this arithmetic): `item_anchor.rs` 517 to about 457;
   `codec.rs` 514 to about 474 with the first seam and about 452 with both; `plan.rs` 520 to about 489 moving
   only `Reexport`, about 449 moving `Reexport` and the predicates, about 333 moving `RefactorKind` whole.
   Wave 2 adds three operation kinds (and `move-fidelity` one field) to `plan.rs`/`refactor_kind.rs` and a codec
   call each; roughly 45-50 lines of variants and docs. A `plan.rs` at 489 or 449 is back over (or on) the
   line after the first two of them. That is why the developer decided (D1, approved 2026-10-05) to move `RefactorKind` whole: the variants
   then land in `plan/refactor_kind.rs`, and `RefactorOp` (which stays in `plan.rs`) takes the new fields.
4. **Every outside caller keeps its path if the move leaves a facade.** `Reexport` is named in 32 other
   files (18 in `src`, 14 in `tests`; 84 `Reexport::`/`repoints_callers` hits in `src` outside `plan.rs`), `RefactorKind` is re-exported by `lib.rs:36`, `owning_package` has one outside caller. The
   `pub use` facade at the old path (the `item_path` precedent already in `plan.rs:122-124`) means no
   consumer file is edited: that is what keeps a mechanical node mechanical, and what keeps the four
   dependents' rebases trivial. Cost: two to three facade lines per file, counted in the predictions above.
5. **Engine choice.** `move_item` with `name` makes the destination a new child module of the file's own
   module (`to` = that module, the new file is `<module dir>/<name>.rs`) and copies the moved items by byte
   range, so comments arrive unchanged (`move_item_acceptance::keeps_the_doc_comment_the_attribute_and_the_inner_comment_of_the_moved_item`).
   `extract_module` is the assist-driven alternative and has an open record of dropping comments and
   of being blind to sibling seams cut by the same plan
   (`docs/dev/todo/2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md`,
   `2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md`). `move_item`
   moves an `impl` block as part of a contiguous run and refuses only a range *inside* one
   (`item_move/outline.rs`); `impl_item_anchor_acceptance.rs` shows the `<Type>` item name for the block
   (for `extract_module`; `move_item` over it is not exercised by any test: a probe, see the changeset).
6. **One lint is predictable.** `rfc3339` is used by `hint_of` and by tests only. After the move its facade
   line, `pub(crate) use file_hint::{hint_of, rfc3339};`, has no non-test user, which rustc reports as an
   unused import under `-D warnings`. Not verified (nothing was built); the changeset treats it as a
   decision to take when it fires.
7. **The measuring instrument has a catch.** `check --budget N` reports "the files the plan's anchors name"
   (`budget::files_named_by`), so it measures a plan, not a tree. After the applies the three plans are
   consumed (an apply rewrites its anchors) and cannot be reused; the closing measurement needs a throwaway
   plan that names the three files. Whether a static `check` accepts such a plan was not run (read-only
   tree); the budget lines are emitted after the findings, so they are printed whatever the findings are.
8. **Two documents already disagree with the tree.** `packages/tddy-code-restructuring/README.md:123-124`
   lists `runner/tidy.rs` as over the budget: its first `#[cfg(test)] mod` opens at line 37, so by the budget
   rule it has 36 production lines. And `docs/signature-assists.md:13`, `docs/signature-rewrites.md:14`,
   `docs/same-crate-moves.md:62` name `plan.rs` as the home of `RefactorKind` and its predicates: stale the
   moment `RefactorKind` moves. Both are wrap-time corrections (package docs go through the changeset).
9. **No test and no consumer needs to change.** Dependents of the crate: `tddy-daemon-rpc`,
   `tddy-index-daemon`, `tddy-tools` (`Cargo.toml` grep); the last two name `tddy_code_restructuring::plan`
   items and `item_anchor::` functions, all reachable through the facades.
10. **Decided 2026-10-05 (developer): `RefactorKind` moves whole (D1) and `RefactorOp` fields are paid for by the node that adds them (D7).**
    `RefactorOp` stays in `plan.rs` (`plan.rs:381`, with `impl RefactorOp` at `:472`); `RefactorKind` (`:147`) and its `impl` (`:285`) go to
    `plan/refactor_kind.rs`. The literal count behind D7, measured on `a77bca29`: `git grep -n 'RefactorOp {' -- packages | wc -l` is 80, of which
    2 are the definitions, 41 are function signatures returning the type, 17 are `..base` struct updates and **20 are full literals in 15 files**
    (cross-check: `git grep -n 'order: Vec::new()' -- packages/tddy-code-restructuring | wc -l` is 20, in 15 files). All 80 are inside
    `tddy-code-restructuring`. This node edits none of them.

## Exploration 1: the whole-work discovery of the `#sharpen` stack — 2026-10-05

**Agent**: parent Grep/Glob/Read (three passes, in the whole-work file)
**Scope**: the whole `#sharpen` work: nine deficiencies, the engine, the wait/spawn code, the test machinery.
Copied in full from `docs/dev/1-WIP/2026-10-05-engine-fixes-whole-work-initial-discovery.md` (its own
headings are demoted two levels so they nest under this one; nothing is edited).

#### Whole-work Combined Conclusions

##### 0. Headline

1. **Every todo's behavioural claim is true of the code on master**, with two qualifications: the doc-link claim
   (that rust-analyzer reports doc-link references) is **unverified** and needs one live probe before B3 is
   designed; the `apply`-hang (D) is **not reproduced and its cause is probably environmental** (see §1 D).
2. **D as titled contradicts a deliberate, written design rule** ("a run waits until the server is ready or until its
   caller stops; there is no budget flag" — `packages/tddy-code-restructuring/README.md:20`;
   `docs/readiness-and-gates.md` "Time spent before the index is ready does not count"; and the open code issue
   `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md` § "If you are about to
   change this code": *"The tool's waiting behaviour is the design … both a timeout and a pipeline destroy that
   signal"*). Two master todos already ask the same developer question and were deferred for it
   (`2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting.md`,
   `2026-10-02-rust-backend-locate-symbol-waits-on-an-empty-outline-with-no-deadline.md`). **D1 cannot be planned
   until the developer chooses** (options in §7). D2 (spawn/exit record) has no such conflict.
3. **Two of the three "new capabilities" mostly exist in `crate_move` and need exposing, not inventing**:
   `crate_move::survey::survey_moved_file` + `crate_move::reexports::followed` already resolve every path a file names,
   headers *and* bodies, through `pub use` facades to the defining path (`SurveyedPath.defined_at`), and
   `crate_move::header::repointed_header` already writes the re-point edits. They are private to `crate_move` and tied to
   a `Move` (origin to destination), so B2 (facade path in moved text) and C1 (`repoint_facade_imports`) are one shared
   resolver in front of that machinery. This is a **real edge: whichever lands first builds the shared helper; the other
   consumes it.**
4. **New operations are mostly engine-internal**: `tddy-tools` names no operation anywhere in its source (grep: only a
   doc example in `tddy-lsp-executor/src/restructure_via_index.rs:33`), so a new op touches `tddy-code-restructuring`
   plus docs; `tddy-tools`/`tddy-index-daemon` are touched only if D changes their output or if A changes routing
   (it does not — see A).
5. **Preparatory restructure: not necessary** (verdict ⚠ During, not ⛔). Measured: `backends/rust.rs` 2,852
   production lines (its remaining seams need the impl-member move the engine cannot do — and C2 does not provide it),
   `plan.rs` 520, `plan/codec.rs` 514, `item_anchor.rs` 517 (all three already past 500, deferred with consent in
   `2026-10-05-restructure-engine-files-past-the-500-line-budget.md`). The precedent that avoids growth is
   `plan/codec/signature_fields.rs` (131 lines): put each new op's field rules in a child module and leave
   `parse_op` one call line richer. A prep node would have to move ~100 lines out of `parse_op` first (it is 198 raw / 137
   code lines) — cheap, but it spends review budget the new ops can avoid; offer it, recommend declining (§4).
6. **Node shape**: n-AB is two capabilities in one node (A and B are file-disjoint) and B is itself three behaviours;
   n-C2 is two operations plus a variant and is too large for one reviewable PR; n-D is two capabilities with
   disjoint files. Proposed split and edges in §6.

##### 1. Per deficiency: where it lives, smallest change, claim verified?

| ID | Todo | Verified? | Lives in | Smallest correct change |
|---|---|---|---|---|
| A | snapshot cannot write a missing header | **Yes** | `plan/codec.rs:65` (`Plan::parse` refuses a first line that is not a header), `runner/entry_points/check_entry_points.rs:71` (`snapshot`) and `:109` (`snapshot_resolving`) | `snapshot` accepts a headerless plan and writes a v2 header built from the ops' anchors' `file` fields (`anchor.file()`, `also` anchors too). ~30-40 lines. |
| B1 | caller's `use` re-pointed instead of full path inlined | **Yes** | `backends/rust/item_move/sites.rs:216` `requalified`, `:237` `Edit::replace(start..site.offset, format!("{qualifier}::"))` | when the written qualifier is a one-segment module bound by a `use` in scope, rewrite that `use` and keep the call's qualifier (new last segment); keep the old `use` if it also serves unmoved items. 2 callers of `edits_for_file`. |
| B2 | facade path inside moved text | **Yes** | `backends/rust/item_move/rebase.rs:99` `path_edit` handles only `self::`/`super::` heads; `assemble.rs:315` `moved_text` | a third pass: for a `crate::<head>` path in the moved region, ask the shared facade resolver; if `defined_at != resolved`, replace. |
| B3 | intra-doc links | **Yes that no code handles it** (no doc-link handling anywhere in `item_move/`, `module_reparent/`, `inline_paths.rs`) | `sites.rs` masks comments via `early_return::masked_to_code`; sites come from `textDocument/references` (`item_move.rs:131 sites_of`) | **unverified**: does rust-analyzer return doc-link positions as references? If yes the fix is to stop masking `///` lines in `edits_for_file`; if no a text pass over `[`path`]` links is needed. One cold-RA probe decides. Also applies to `reparent_module` (`module_reparent/assemble.rs:153` calls the same `edits_for_file`). |
| C1 | `repoint_facade_imports` | **Yes** (grouped `use` is the hard part: `header::repointed_header` refuses a group "whose members would need different qualifiers"; `sites.rs:247 rewrite_statement` already splits a group) | new op; resolver = `crate_move/reexports.rs` `followed` (private mod, `crate_move.rs:284`) + `crate_move/survey.rs:60 survey_moved_file` | survey the anchored file/module with origin = its own crate, rewrite each path whose `defined_at` differs from `resolved`; split grouped `use`. Text + manifests only: **no rust-analyzer needed** for resolution (textual, cycle-safe, path-dependency aware). |
| C2a | `repoint_call` | **Yes** | single-call form: same shape as `backends/rust/signature_rewrites/call_site.rs` (item anchor + relative range over one call, parsed with `syn`, text-only); bulk form needs `item_move.rs:131 sites_of` (references) | new op + two new `RefactorOp` fields (callee/receiver text); bulk = references then one call-site edit per site. |
| C2b | `retarget_impl` (+ delegator) | **Yes** (`plan-schema.md`: "a member of an `impl` cannot move alone", `item_move/outline.rs:115`; `signature.rs:36` refuses `self`) | new op; header rewrite is text; subset split uses `outline.rs` anchors | rewrite `impl Old {` to `impl New {`, split the block at the anchored members, re-point `Old::f` paths inside, add the `use`. Delegator = a variant that leaves `fn m(&self,..) { self.new().m(..) }`. |
| D1 | `apply` waits forever | **Behaviour confirmed in code; cause not** | `backends/rust/readiness.rs:142 await_answer` (unbounded unless `bound`), called by `rename_symbol` (`rust.rs:2049`) via `wait_until_resolved`; loop ends only through `rust.rs:620 keep_waiting` (cancel token) | see §7: a stall bound or a heartbeat; **developer decision first**. |
| D2 | spawn/exit record | **Yes** (the journal records `WorkspaceEdit`s only; spawn sites enumerated in Exploration 3) | spawns: `backends/rust.rs:685`, `runner/compile_gate.rs:253`, `runner/tidy.rs:551,565`, `runner/tidy/format.rs:76`, `apply.rs:39,177,189`, `tddy-lsp/src/server_body.rs:55`, `run-index-daemon` | a process-record module + a hook at each spawn; **children of rust-analyzer (build scripts, proc-macro server) are spawned by RA and are not observable by any of this** — the todo's "every build script" cannot be met. |

**D, what is and is not established.** The todo's last lines — `waiting for type inference at the anchor` /
`working: build script num-bigint run`, 0% CPU, daemon alive — match `await_answer` looping because
`chatter.loading()` (`chatter.rs:233`: `reported_status && !quiescent`) stays true: rust-analyzer reported
`quiescent:false` while running a build script and never finished. The sibling todo records the developer's endpoint
protection blocking a freshly written script in the same window; a blocked build script leaves RA non-quiescent
forever, which is exactly this signature. **Hypothesis, not established.** `check --deep` "answered" earlier because
`indexed` was already true then and the edits that followed (seven files, picked up through the daemon's
`didChangeWatchedFiles`) restarted a load. The existing bounded wait (`READY_HOVER_BOUND`, 30 s) only covers a
*ready* index with a null hover.

##### 2. Shared machinery and real edges

Shared files each node would edit (production unless noted):

| File (production lines) | A | B1 | B2 | B3 | C1 | C2a | C2b | D1 | D2 |
|---|---|---|---|---|---|---|---|---|---|
| `plan.rs` (520, over) | | | | | kind | kind + 2 fields | kind + field | | |
| `plan/codec.rs` (514, over) | | | | | 1 call | 1 call | 1 call | | |
| `backends/rust.rs` (2,852, over) | | | | | +~12 | +~12 | +~12 | maybe | maybe |
| `backends/rust/item_move/sites.rs` (333) | | **edit** | | **edit** | reads helpers | | | | |
| `backends/rust/item_move/rebase.rs` (191) / `assemble.rs` (429) | | | **edit** | | | | | | |
| `item_move/text.rs` (255): `use_statements`, `split_use`, `members_of` (in sites.rs) | | edit | | | reuse (widen visibility) | | reuse | | |
| `crate_move/reexports.rs`, `survey.rs`, `header.rs` | | | reuse | | reuse | | | | |
| `check_entry_points.rs` (356), `item_anchor.rs` (517, over) | edit | | | | | | | | |
| `backends/rust/readiness.rs` (290), `rust.rs` wait loops | | | | | | | | **edit** | |
| `tddy-lsp/src/server_body.rs`, `run-index-daemon`, `.agents/skills/code-restructuring/SKILL.md` | | | | | | | | | **edit** (and open #586 edits the last two) |
| docs: `plan-schema.md`, `SKILL.md`, `docs/ft/coder/rust-code-restructuring.md`, package `docs/same-crate-moves.md`, README op counts | A | B | B | B | C1 | C2 | C2 | D1 | D2 |

Real dependency edges:
- **B2 and C1 share one resolver** (`reexports::followed` made reachable from `backends/rust`, wrapped as
  `facade_target(workspace, package, path)`). Edge: first of them builds it. Because C1 also needs grouped-`use`
  splitting that B1 touches (`rewrite_statement`, `members_of`), order **B1, B2, then C1**.
- **C2b and C2a are independent at the code level** (retarget header/impl-block text vs call-site rewriting) but
  *used* together: retargeting a member breaks every caller until they are re-pointed, which is why the todos make
  the delegator an option. They want to be applied as one transactional `group`.
- **A has no edge** to anything (`check_entry_points.rs` + codec header read only).
- **D1 depends on the developer's decision; D2 has no edge to code nodes**, but its files overlap open PR #586.
- **Every new-op node edits `plan.rs`/`codec.rs`/`rust.rs`** (enum variants, SUPPORTED, check arm, resolve arm,
  `RefactorOp` fields): C1, C2a, C2b conflict mechanically with each other there — keep them in a linear stack.

##### 3. Vertical slice of an operation, and test level

The exemplar is `cb50ab5c` (#584, `move_item` + `reparent_module`, 62 files in the engine package). A new operation touches:
`plan.rs` (`RefactorKind` variant + docs, `RefactorOp` fields, `deny_unknown_fields` means every field must be declared),
`plan/codec.rs` (`parse_op` rule chain; put rules in a child module like `codec/signature_fields.rs`),
`backends/rust.rs` (`SUPPORTED` at `:66` — `[RefactorKind; 22]` becomes 23…, `check` arm at `:1011`, `resolve` arm at `:1168`),
a new `backends/rust/<op>/` module, docs (plan-schema table row + SKILL.md counts "twenty-two operations, ten
subcommands" + `docs/ft/coder/rust-code-restructuring.md` + package README + `docs/same-crate-moves.md`-style page),
`verify` (see risk below). Not touched by an op: `restructure_args.rs`, `restructure_cli.rs`, `tddy-tools`, the daemon proto.
The warm-routing edits in `cb50ab5c` were for `warm`/`snapshot`, not for the ops.

Test levels (all measured from the repo, nothing run):
- **Live rust-analyzer acceptance** (`#[tokio::test(flavor = "multi_thread")]`, fixture crate written to a temp dir by
  `tests/harness/mod.rs` + `tests/same_crate/mod.rs`, `cargo check` as the assertion; harness `ONE_SERVER_AT_A_TIME`
  mutex; "tens of seconds each", `SKILL.md:105`). Style for B1-B3, C2a-bulk, C2b. A **new live binary must be added to
  `.config/rust-e2e.filterset`** (rule in the file header); note `move_item_*` and `reparent_module_*` binaries are **not
  listed there today** (existing gap, ℹ not ours). Prefer adding tests to the existing `move_item_*_acceptance.rs` files
  when they fit to avoid new binaries.
- **Library-level, no server**: `tests/snapshot_rewrites_the_header.rs` style (`runner::snapshot(root, Options)`,
  milliseconds) for A; `signature_rewrites_acceptance.rs` (451 lines) style for C2a single-call, which answers "before
  any server is spawned". C1 resolution is textual, so most of it is testable without RA; the apply path is live.
- **Fake LSP** (`tddy-lsp/tests/bin/fake_lsp.rs`, `--cold-hovers`, no real RA; `cancellation_acceptance.rs`,
  `wedged_request_acceptance.rs`) for D1: fast; needs `fake_lsp` to learn a scripted `quiescent:false` that never ends
  (today it only has `--loads-crate-graph` with `quiescent:true`, `fake_lsp.rs:317`).
- **CLI child-process** (`tddy-tools/tests/restructure_cli_acceptance.rs`, `assert_cmd`, fast) for A end to end.

**Verify risk (design question, not a bug):** `verify --against` excuses re-points only by deleting lowercase module
qualifiers (`verify.rs` docs, passes 1-5). C1 and B2 produce `crate::config::X` → `other::config::X` (accounted for);
`use` items are scaffolding. **C2a (`self.f(x)` → `self.peer.f(x)` or a callee with new arguments) and C2b
(`impl A` → `impl B`) change tokens verify cannot pair**, so `restructure verify` will report them. Decide whether
verify learns these ops (it takes no plan, only a git ref) or the skill states they are outside its proof.

##### 4. Step 2b — both records, every package in scope

Packages: `tddy-code-restructuring`, `tddy-tools`, `tddy-index-daemon`, plus `tddy-lsp` (D2; **no
`docs/code-issues/` directory — not analyzed, not the same as clean**; name it in the changeset).

| Record | Verdict | Notes |
|---|---|---|
| `tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` (2,852 prod lines; `Restructure: required`; deferred) | ⚠ **During** | `Claimed by`: none (no such field; **no open PR touches the file** — `gh pr list` shows #532-#536 (docs/todos + lifecycle), #586 (`run-index-daemon`, `SKILL.md`, a todo)). Each op adds ~12 wiring lines (`cb50ab5c`: +24 for two ops). Constraint: logic in siblings, re-measure and append a history row. Its remaining seams need the impl-member move the engine lacks; C2b does **not** supply it (it retargets between types, not across files). |
| `…/oversized-file-test-binary.md` (967) | — | `crate_move/test_binary.rs`; none of these nodes edit it. |
| `…/complexity-rust-facade-lines.md` (`backends/rust/facade.rs:151`, nesting 5) | — | `facade.rs` = the *left-behind* facade, a different meaning of "facade" from C1/B2's `pub use` re-export. Not touched. |
| `…/dead-code-plan-filehint-modified.md` | ⚠ **During** (A) | A writes v2 headers through `hint_of` (`plan/codec.rs:219`): do not add a second writer of `modified`; either use `hint_of` or delete the field. |
| `…/broken-restructure-anchors-empty-outline.md` | — / 🚧 check | the only file matching `Claimed by:`; its value is `none` (#537 merged 2026-10-02, `gh pr view 537`: MERGED). **Not a live claim.** Relevant only to A's wording: do not route A through `anchors`. |
| `tddy-tools/docs/code-issues/*` (4: `run_call_tool`, `subagent_new_session_tool`, `take_a_turn`, `server.rs` 2,677) | — | none of these nodes touch `cli.rs:525` or `server.rs`. `index_client.rs`/`index_console.rs` are the restructure front end and have no record. |
| `tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md` | ⚠ **During / blocking-by-decision for D1** | open, unclaimed. States the waiting design; says an interrupted warm leaves the latch set. D1's failure path must not leave `indexed` set, and must not contradict the record without the developer's decision. Item 3 there (`ensure_indexed`, empty outline vs answered) is adjacent. |
| `…/complexity-warm-narrate-until-loaded.md` (`warm.rs:102`, nesting 5) | ⚠ During (D1, if it touches `warm.rs`) | |

`grep -rl 'Claimed by:'` over the three packages: one hit (above, `none`). **No 🚧 claimed issue is in this change's
path, so no wait-or-proceed fork exists.** `docs/dev/1-WIP/` holds one restructure changeset still marked 🚧
(`2026-09-17-restructure-refusal-truth-and-authoring-gates.md`, all milestones `[x]` — it introduced `snapshot`); looks
unwrapped rather than active; ℹ ask whether it is stale before wrap.

`docs/dev/todo/` scan (237 files; 234 not marked Resolved; 69 match restructure/index-daemon). Read and classified:

| Todo (master) | Verdict |
|---|---|
| `2026-10-05-restructure-engine-files-past-the-500-line-budget.md` (`plan.rs` 520, `codec.rs` 514, `item_anchor.rs` 517) | ⚠ **During**: C1/C2a/C2b/A add to these files; keep new rules in child modules. Not ✅ unless a prep node does the split. |
| `2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md` | ⚠ During (its stated blocker, the open `#live-plan` stack, has landed: #567 15/15; the split is now blocked only by the engine's missing impl-member seam). |
| `2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting.md`, `2026-10-02-rust-backend-locate-symbol-waits-on-an-empty-outline-with-no-deadline.md` | ℹ **Answered by D1's decision**; ✅ RESOLVED HERE only if the chosen bound covers `settled_outline` and `locate_symbol` too (a bound confined to `await_answer` does not close them). |
| `2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md` | ⚠ **Reconsider**: "the same family" as C2; `self.<field>` to `state.<field>` is a field read, not a call, so `repoint_call` as described does **not** close it. Decide whether C2a is generalised ("repoint receiver") — then ✅. |
| `2026-10-04-restructure-move-item-copies-the-whole-use-header.md`, `…-reexport-outside-limits.md`, `…-reparent-module-first-cut-limits.md`, `…-same-crate-moves-limits-found-moving-lifecycle.md` | — (B1/B2 must not widen these; B3 adds doc links to the `reparent_module` behaviour page). |
| `2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md` § 6 (`verify` reflow noise) | ⚠ During (verify risk, §3). |
| `2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing.md` | — (A is not the live-plan snapshot RPC; but `snapshot_resolving` must also accept a headerless plan, see A). |
| `2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md` | — (range-plan rebase, different). |

**The 9 todos named in the brief are not on master** (they live on #532 `feature/carve/lifecycle-ports-agents` and on #586
`fix/index-daemon-log-history`), so they cannot be linked or deleted by a PR off master until those merge. Process
consequence for `## Prerequisites`: reference them by branch/PR; the ✅ RESOLVED HERE deletion happens at wrap only if
they have reached master by then. Two of them (the facade-import and moved-signature facade-path todos) carry a stale link to
`2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md`, which #540 deleted
(`git log --all` shows `c5d00566`/`b0e6de6e`); its replacement is `packages/tddy-code-restructuring/docs/path-survey.md`.

**Preparatory restructure (the bar).** Difficulty named: `parse_op` (`plan/codec.rs:295`-`493`) is one rule chain
(198 raw / 137 code lines, under the 150 function budget by code lines) in a file at 514; each new op appends rules and
`plan.rs` appends enum variants and fields (`RefactorKind` is 138 lines of mostly docs, `plan.rs:147`-`285`). Smaller than the
change? Yes (~100 lines moved). Behaviour-preserving? Yes. Expressible? Partly: `extract_method` (inline `if` blocks)
then `move_item`. Blast radius measured: `parse_op` has one caller (`Plan::parse`), the plan unit tests are
`plan.rs:521`-`1679` (~1,150 lines). **But it is avoidable** (child module per op, as `signature_fields.rs` did), and
`RefactorKind` itself cannot be split. Recommendation: **do not add a prep node; record the constraint; if the
developer wants the file-length gate green, add it before C1 as a mechanical node** (`plan/codec.rs` rules out,
`plan.rs` `Reexport` + predicates out). `rust.rs` cannot be prepped by the engine at all.

##### 5. Existing todos these nodes could resolve

See table above: the two `no deadline` todos (conditional on D1's bound), the `state parameter` todo (conditional on a
generalised C2a), and the nine brief todos once on master. Nothing else on master is closed by A, B or C1.

##### 6. Proposed edges and waves (linear stack, green in waves)

Recommended nodes (capability-based):

| Node | Capability | Slice |
|---|---|---|
| n-A | `snapshot` writes a missing header | library + `snapshot_resolving` + CLI test + SKILL step 6 + plan-schema header section; no server |
| n-B | `move_item`/`reparent_module` fidelity: B1 use re-point, B2 facade path in moved text (**builds the shared resolver**), B3 doc links | one node is acceptable (one pipeline, `item_move/` only) **if** the B3 probe says doc links are references; split B3 off if it needs a text pass |
| n-C1 | `repoint_facade_imports` | new op, vertical slice; consumes B2's resolver |
| n-C2a | `repoint_call` (single call, then bulk over references) | new op + 2 fields |
| n-C2b | `retarget_impl` (+ delegator variant) | new op + field; delegator may be its own node if the block-split is already large |
| n-D1 | `apply` stall handling | **blocked on developer decision** |
| n-D2 | spawn/exit record | engine + `tddy-lsp` + `run-index-daemon`; after #586 merges |

Edges: B before C1 (resolver, `rewrite_statement`/`members_of` visibility). C2a and C2b independent of C1 and of each
other in code; keep C2a before C2b so a retarget plan can carry its call re-points. A, D1, D2 are edge-free.
Waves for green: **wave 1** A and B (disjoint files, parallel worktrees); **wave 2** C1; **wave 3** C2a then C2b
(each edits `plan.rs`/`codec.rs`/`rust.rs`, so they cannot overlap C1 in time without conflicts); D1/D2 any time (D2
after #586). In the gh stack they remain linear in the order A, B, C1, C2a, C2b, D2 (D1 when decided).

##### 7. Decisions the developer must make before the changeset

1. **D1 shape** (ASK): (a) heartbeat only — a periodic "still waiting at `<stage>`, server last said `<x>` for N s" line, no new
   budget, consistent with README:20; (b) a **stall bound** — fail with the stage when the server's progress has not
   changed for N minutes (new bound, contradicts the readiness doc's "time before ready does not count" and the poisoned-latch
   record's warning; answers the two `no deadline` todos only if applied to all outline waits); (c) opt-in bound
   by agent-driven config rather than a flag (memory: prefer agent-driven config over flags). Also: whether the failure
   must clear the `indexed` latch.
2. **verify and receiver rewrites** (C2): extend `verify` or document that C2 ops are outside its proof.
3. **C2a scope**: calls only, or any receiver including `self.<field>` (closes the `state parameter` todo).
4. **Prep node** for `plan.rs`/`codec.rs`: decline (recommended) or add.
5. **B3**: run the cold-RA probe now or discover it in red.

#### Whole-work Exploration 1: the deficiency records and the deferred-work scan — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: the nine #532/#586 todos, both Step 2b records for the packages in scope, open PRs.

##### Sequence

1. Read `.agents/skills/planning/references/initial-discovery.md`, `planning-phase.md` Step 2/2b,
   `.agents/skills/deferred-work/references/planning-cross-check.md`, `.agents/skills/code-restructuring/SKILL.md`,
   `references/plan-schema.md` (lines 1-200 plus the operations table).
2. `git diff --name-only origin/master...feature/carve/lifecycle-ports-agents -- docs/dev/todo` — 9 files (8 added + the modified
   `2026-09-24-lifecycle-modules-to-re-parent-by-hand.md`, whose change is a status update unrelated to these nodes).
3. `git show <branch>:<file>` for each into the scratchpad; `git show fix/index-daemon-log-history:docs/dev/todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md`.
4. `git diff --name-only origin/master...feature/carve/lifecycle-ports-agents | grep -E "restructur|index-daemon|tddy-tools|code-issues|skills"` — only the nine todos; #532 changes no engine file.
5. `git diff --stat origin/master...fix/index-daemon-log-history` — `.agents/skills/code-restructuring/SKILL.md` (+6), the todo, `run-index-daemon` (+35/-2).
6. `gh pr list --state open` — #532-#536 lifecycle stack, #586 (draft, base master).
7. `ls packages/{tddy-code-restructuring,tddy-tools,tddy-index-daemon}/docs/code-issues`; `ls packages/tddy-lsp/docs/code-issues` (absent).
8. Read all five restructuring code issues, all four `tddy-tools` issues (heads), both `tddy-index-daemon` issues.
9. `grep -rn "Claimed by"` over the four packages; `gh pr view 537 --json state,mergedAt,baseRefName`.
10. `ls docs/dev/todo | wc -l`; `grep -rl -iE 'restructure|code-restructuring|index-daemon|index daemon' docs/dev/todo`; read
    `engine-files-past-the-500-line-budget`, `rust-backend-grows-with-every-live-plan-node`, `same-crate-moves-limits`,
    `move-item-copies-the-whole-use-header`, `reexport-outside-limits`, `snapshot-cannot-rebase-a-stale-plan`,
    `live-plans-three-gaps`, `locate-symbol-waits`, `server-status`, `state-parameter`.
11. `ls docs/dev/1-WIP`; read the head of `2026-09-17-restructure-refusal-truth-and-authoring-gates.md`.

##### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `Claimed by` | `packages/*/docs/code-issues` (four packages) | only `broken-restructure-anchors-empty-outline.md:6` → value is `none` |
| Grep | `restructure\|…` | `docs/dev/todo` | 69 files; classified in Combined Conclusions §4 |
| Bash | `gh pr list --state open` | repo | #586 touches `run-index-daemon` + `SKILL.md`; #532 touches only todos |

##### Inspected files

###### `docs/dev/todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md` (on #532)

**Why**: claim D1.
**Excerpt**:

```
restructure check <plan> --deep      -> "no findings"            (finished)
restructure apply <plan>             -> did not return
indexing (+0ms): waiting for type inference at the anchor
indexing (+2.0s): working: build script num-bigint run
… "I did not investigate the cause."  / "Not reduced."
```

###### `docs/dev/todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md` (on #586)

**Why**: claim D2. States CrowdStrike Falcon "Malicious script was blocked" in the same window; asks for (1) a spawn log of every process
the engine, CLI and daemon start, with exit status or signal; (2) the daemon's own exit recorded by a parent that is not the pid `--stop`
signals; (3) a per-operation line naming the op id. "the engine change is its own node".

###### `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md` (on #532)

**Excerpt**: `Error: the index daemon refused this run (InvalidArgument): plan is malformed: first line must be a snapshot header`;
"A v2 header is only hints for item anchors, so the engine can compute it from the plan's own anchors' `file` fields."

###### the other six #532 todos

`facade-import` (B2/C1: `crate::config` is `pub use tddy_daemon_kernel::config`; 12 files, 30 lines, 4 paths in grouped `use`),
`move-item-copies-a-moved-signature-s-facade-path` (`agent_roster.rs:186` holds `crate::config::DaemonConfig` after a move),
`…-intra-doc-links` (`handler_state.rs:106`), `…-writes-a-caller-re-point-as-a-full-path`
(`crate::connection_service::agent_roster::split_forward_deadline(&self.config)`), `no-operation-re-points-a-calls-receiver-or-writes-a-delegator`
(28 call sites + 7 wrappers by hand), `no-operation-retargets-an-impl-to-another-type` (22 methods; `change_param_type` on `self` refused:
`this seam cannot be cut here: self is not a parameter of the function the anchor names`).

###### `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md`

**Why**: the standing record of the waiting design.
**Excerpt**:

```
## If you are about to change this code
Never wrap a `restructure` call in `timeout` and never pipe it. The tool's waiting behaviour is the
design — a run waits until the server is ready or until you stop it — and both a timeout and a
pipeline destroy that signal.
```

###### `docs/dev/todo/2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting.md`

**Excerpt**: "A bounded wait would be a budget, which the restructuring library deliberately withdrew in favour of cancellation … It needs the developer to choose."

###### `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md`

2,853 production lines at 2026-10-05; "The impl-member seams remain; none was cut"; each of the last three growth rows is wiring only.

##### Findings

- All nine todos describe real behaviour; none is already fixed on master.
- The 9 todos are not on master; two link a todo deleted by #540.
- No claimed code issue is in the path; the only `Claimed by:` hit is `none`.
- D conflicts with a written design rule and with two deferred todos.

#### Whole-work Exploration 2: the engine — snapshot, move_item, facades, op registration — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: `packages/tddy-code-restructuring`, `packages/tddy-tools`, `packages/tddy-index-daemon`.

##### Sequence

1. `grep -rn "first line must be a snapshot header"` in `src` — `plan/codec.rs:65`.
2. Read `runner/entry_points/check_entry_points.rs:40-200` (`snapshot`, `snapshot_resolving`, `check`).
3. Read `plan/codec.rs:40-110` (`Plan::parse`, `rehashed_header`), `:255-514` (`parse_op` and rules).
4. Read `item_anchor.rs:247-254` (`plan_file_has_item_anchors`), `tddy-tools/src/index_client.rs:40-75`, `tddy-index-daemon/src/queries.rs:115-170`.
5. Read `tests/snapshot_rewrites_the_header.rs:1-80` (style).
6. Read `backends/rust/item_move.rs` (all), `item_move/sites.rs:1-333`, `item_move/assemble.rs` (all), `item_move/rebase.rs`, `item_move/bindings.rs`.
7. Grep intra-doc / doc link handling in `item_move*`, `module_reparent*`, `inline_paths.rs`, `crate_move/*.rs` — none.
8. Read `backends/rust/early_return.rs:266-295` (`masked_to_code` blanks comments).
9. Read `crate_move/module_home.rs:120-330` (`defining_crate`), `crate_move/reexports.rs` (`followed`), `crate_move/survey.rs:1-169`, `crate_move/header.rs:1-130`, `docs/path-survey.md:1-60`, `crate_move/destination.rs`.
10. `grep -rn 'MoveItem' packages --include='*.rs'` (outside item_move/tests) — `plan.rs:231`, `codec.rs:349,363,400`, `rust.rs:77,1011,1168`.
11. `git show --stat=200 cb50ab5c -- packages/tddy-code-restructuring .agents docs/ft` and `git show cb50ab5c -- <six small files>` — the op slice; the warm edits were unrelated.
12. `grep -rln 'move_item\|"extract_module"' packages` outside engine — only a doc example in `tddy-lsp-executor`.
13. Read `plan.rs:147-330` (`RefactorKind`), `:376-450` (`RefactorOp`); measure `parse_op` (awk) and production lines with `prod.sh` (awk: up to the first `#[cfg(test)]` directly followed by `mod`).
14. Read `verify.rs:1-120`, `backends/rust/signature_rewrites/call_site.rs:1-80`, `docs/signature-rewrites.md:1-70`.
15. Count callers: `survey_moved_file(` 2 production (+1 test) callers; `edits_for_file(` 2 callers; `Plan::parse(` callers.

##### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `first line must be a snapshot header` | `src` | `plan/codec.rs:65` |
| Grep | `MoveItem` | packages, minus `item_move/`, tests | `plan.rs:231`, `codec.rs:349,363,400`, `rust.rs:77,1011,1168` |
| Grep | `reexports::followed\|followed\b` | `crate_move*` | `crate_move/survey.rs:101` is the only production call |
| Grep | `survey_moved_file(` | `src` | `header.rs:73`, `preconditions.rs:97` |
| Grep | `edits_for_file(` | `src` | `item_move/assemble.rs:308`, `module_reparent/assemble.rs:153` |
| Grep | `intra-doc\|doc link` | item_move, module_reparent, inline_paths, crate_move | none |
| Awk | production lines | 36 files | `rust.rs` 2852, `plan.rs` 520, `codec.rs` 514, `item_anchor.rs` 517, `readiness.rs` 290, `sites.rs` 333, `assemble.rs` 429, `rebase.rs` 191, `header.rs` 389, `survey.rs` 169, `reexports.rs` 259, `journal.rs` 460 |

##### Inspected files

###### `packages/tddy-code-restructuring/src/plan/codec.rs`

**Why**: A — where the header is required; the rule chain every op joins.
**Excerpt**:

```rust
let header: SnapshotHeader = serde_json::from_str(header)
    .map_err(|_| malformed("first line must be a snapshot header"))?;          // codec.rs:65
// parse_op: 295..493, then `mod signature_fields;` (131 lines, the precedent) at 494
if op.op == RefactorKind::MoveItem { names_a_destination_and_anchors_by_item(&op, "move_item", …)?; }   // :363
```

###### `packages/tddy-code-restructuring/src/runner/entry_points/check_entry_points.rs`

**Why**: A — `snapshot` reads the plan with `Plan::parse`; `snapshot_resolving` first calls `read_plan`.
**Excerpt**:

```rust
pub fn snapshot(root: &Path, options: Options) -> Result<SnapshotRewrite> {      // :71
    let path = options.plan()?;
    let text = std::fs::read_to_string(&path)?;
    let plan = Plan::parse(&text)?;                // fails for a headerless plan
    let header = plan.rehashed_header(root)?;       // "Only the paths the header already names"
pub fn snapshot_resolving(…) {                      // :109
    if !has_item_anchors(&read_plan(&path)?) { return snapshot(root, options); }   // also fails headerless
```

`plan_file_has_item_anchors` (`item_anchor.rs:249`) swallows the parse error and returns false, so the CLI
(`index_client.rs:57-60`) and the daemon (`queries.rs:123-170`) both route a headerless plan to the in-process
`snapshot` without a server: **A needs no routing, proto or `tddy-tools` change**, but the daemon's `snapshot_resolving`
call must not hit `read_plan` first.

###### `packages/tddy-code-restructuring/src/backends/rust/item_move/sites.rs`

**Why**: B1/B3.
**Excerpt**:

```rust
// requalified (:216): only a qualifier can be re-pointed
let start = qualifier_start(text, site.offset);
…
Ok(Some(Edit::replace(start..site.offset, format!("{qualifier}::"))))          // :237 — the full crate:: path
// edits_for_file (:89): `let masked = masked_to_code(text);` — comments are blanked before sites are read
```

`rewrite_statement` (`:247`) rewrites a `use`, splitting a group (`members_of`) so a name leaves it; a nested group is refused.

###### `packages/tddy-code-restructuring/src/backends/rust/item_move/rebase.rs`

**Why**: B2 — the only pass over the moved text.
**Excerpt**: `path_edit` (`:99`) starts at `at` and loops `if rest.starts_with("super::") … else if rest.starts_with("self::") … else break; if chain == 0 { return None; }` — a `crate::` head returns `None`.

###### `packages/tddy-code-restructuring/src/crate_move/{survey,reexports,header}.rs`, `docs/path-survey.md`

**Why**: the existing facade resolution.
**Excerpt**:

```rust
// survey.rs:101
let defined_at = reexports::followed(workspace, origin, &resolved)?;
// SurveyedPath { written, resolved, defining_crate, defined_at, in_test, in_body, site }
// header.rs docs: "Refuses a `use` group whose members would need different qualifiers, because one prefix is all a group has"
// reexports.rs docs: "The walk reads sources and nothing else … explicit `use`, then the module's globs, each of which has to confirm the name"
```

Visibility: `mod reexports;` is private (`crate_move.rs:284`), `survey` private (`:287`); both are `pub(crate)` items inside private modules.

###### `packages/tddy-code-restructuring/src/verify.rs`

**Why**: whether new ops stay accountable.
**Excerpt**: "Re-point pairing: … equal once lowercase module qualifiers are deleted (`f(` becoming `m::f(`)"; "It cannot excuse a renamed callee, a changed or dropped argument…". `use` items are scaffolding, wholly.

###### `packages/tddy-code-restructuring/src/plan.rs`

`RefactorKind` `:147-283` (138 lines), `impl RefactorKind` `:285`, `Reexport` `:334`, `RefactorOp` `:381` with `#[serde(deny_unknown_fields)]`; production 520 before `mod tests` at `:521`.

###### `git show cb50ab5c` (the exemplar slice)

`rust.rs` +24 (module decls, SUPPORTED 20→22, check arms, resolve arms); `plan.rs` +184; `plan/codec.rs` +79; `console.rs` +50;
`item_anchor.rs` +58; `restructure_args.rs`/`restructure_cli.rs`/`runner/entry_points.rs`/`runner/options.rs`/`lib.rs` changes were for
`warm`; 10 acceptance binaries; docs: `SKILL.md` +49, `plan-schema.md` +82, `docs/ft/coder/rust-code-restructuring.md` +116, package README +15, `docs/same-crate-moves.md` (110).

##### Findings

- A: header creation belongs in `snapshot` (library). The daemon and CLI already route correctly.
- B1: 2 callers of `edits_for_file`; fix confined to `sites.rs` (+ `text.rs` helper visibility).
- B2: only `rebase.rs`'s `path_edit` reads the moved text; facade resolution exists in `crate_move` and is private.
- B3: nothing handles doc links; whether RA returns them as references is unknown.
- C1/B2 share a resolver; C2 shares call-site parsing (`signature_rewrites/call_site.rs`) and `sites_of` references.
- New op slice is engine-only plus docs; `tddy-tools` is generic.
- `verify` cannot account for receiver/impl-header rewrites.

#### Whole-work Exploration 3: the wait, the spawns, and the test machinery — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: D1/D2 code, test harness and CI registration.

##### Sequence

1. `grep -rn "waiting for type inference"` in `src` — `readiness.rs:148` only.
2. Read `backends/rust/readiness.rs:1-290`, `rust.rs:595-670` (`keep_waiting`, `incomplete_index`), `:780-830` (`request_settled`), `:2018-2120` (`rename_symbol`, `locate_symbol`), `chatter.rs:195-290`.
3. Read `lib.rs:160-215` (`ServerNotSettled`, `IndexingIncomplete`, `CallerStopped`), `README.md:18-22`, `docs/readiness-and-gates.md:1-120`.
4. Read `tddy-index-daemon/src/apply.rs` (all), `src/warm.rs:60-160`.
5. `grep -rn "Command::new\|process::Command\|\.spawn()"` over engine src, index-daemon src, lsp src, tools restructure files, `run-index-daemon`; read `tddy-lsp/src/server_body.rs:40-120`, `rust.rs:675-730`.
6. `grep -n "setsid\|nohup\|history\|nix develop" run-index-daemon`.
7. Read `tests/wedged_request_acceptance.rs:1-40`, `tests/cancellation_acceptance.rs:1-30`, `tddy-lsp/tests/bin/fake_lsp.rs` (grep `serverStatus|quiescent`).
8. Read `tests/move_item_acceptance.rs:1-110`, `tests/same_crate/mod.rs:1-120`, `tests/harness/mod.rs` (fn index, `ONE_SERVER_AT_A_TIME` at `:57`).
9. Read `.config/nextest.toml`, `.config/rust-e2e.filterset`, `scripts/nextest-filterset.ts` header.
10. Read `tddy-tools/tests/restructure_cli_acceptance.rs:1-40`, `index_daemon_client_acceptance.rs:1-30`.

##### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `waiting for type inference` | engine `src` | `readiness.rs:148` |
| Grep | `self.wait_until_resolved(uri, &position)` | `rust.rs` | `:2004`, `:2049` (rename_symbol), `:2142` (rename_placeholder) |
| Grep | `Command::new\|\.spawn()` | engine, daemon, lsp, tools | `rust.rs:685,706`; `compile_gate.rs:253,259`; `tidy.rs:551,565`; `tidy/format.rs:76`; `apply.rs:39,177,189`; `tddy-lsp/server_body.rs:55,70`; tools `cli.rs:595` (unrelated) |
| Grep | `serverStatus` | `fake_lsp.rs` | `:317` (`quiescent: true` only) |
| Grep | `move_item` in `.config` | | none: `move_item_*`/`reparent_module_*` binaries are not in `rust-e2e.filterset` |

##### Inspected files

###### `packages/tddy-code-restructuring/src/backends/rust/readiness.rs`

**Why**: D1 — the unbounded loop.
**Excerpt**:

```rust
fn await_answer(&mut self, uri, position, bound: Option<Duration>) -> Result<Answerable> {   // :142
    (self.progress)("waiting for type inference at the anchor");
    loop {
        let hover = self.request_settled("textDocument/hover", …)?;
        if !hover.is_null() && !self.chatter.loading() { … return Ok(Answerable::Ready); }
        …                                   // inactive-code / unlinked-file only when indexed && !loading
        if hover.is_null() && self.indexed && !self.chatter.loading() { … silent_past(silent_for, bound) … }
        if !self.keep_waiting(INDEXING_POLL) { return Err(self.incomplete_index(started.elapsed())); }
    }
}
// READY_HOVER_BOUND = 30s: "Time spent before the index is ready does not count"
```

`keep_waiting` (`rust.rs:620`) returns false only when `self.cancel.is_cancelled()`; `incomplete_index` already carries `chatter.how_far()`, so a cancelled wait names the stage — the failure is not the message but that nothing cancels.

###### `packages/tddy-index-daemon/src/apply.rs`

The daemon drives its own apply loop (`apply_held_plan`), calling the same backends through `runner::registry_for`; cancellation is checked between operations and inside the waits. A fix in `readiness.rs` therefore serves both the cold and the warm path.

###### Spawn sites

Engine: `rust.rs:685` rust-analyzer; `compile_gate.rs:253` `cargo check` (already killed on cancel, `:273-285`); `tidy.rs:551,565` cargo; `tidy/format.rs:76` rustfmt; `apply.rs:39,177,189,227..` git. Daemon-side rust-analyzer: `tddy-lsp/src/server_body.rs:55` (`register_child_pid`). Daemon process: `run-index-daemon` (`setsid`, nix env capture `nix develop … env -0`). `journal.rs` (460 production lines) records only `WorkspaceEdit`s.

###### Tests

`tests/same_crate/mod.rs`: fixtures are one package `app`, ops built from JSON; `moving_items(…)` helpers; live RA per test. `tests/snapshot_rewrites_the_header.rs`: library-level, temp git workspace, `runner::snapshot`. `tddy-tools/tests/restructure_cli_acceptance.rs`: `assert_cmd` against the real binary, no RA.

##### Findings

- The hang is the unbounded not-ready branch of `await_answer`; cancellation is the only exit by design.
- A fix serves cold and warm paths from one place; the daemon failure path must also leave no poisoned latch.
- Spawn sites are enumerable in six files plus the launcher script; RA's own children are out of reach.
- Test levels: fake LSP for D1, live RA for B/C2b, library for A, CLI for A end to end; a new live binary needs `rust-e2e.filterset`.


## Exploration 2: the three files, their seams, their callers and the budget instrument — 2026-10-05

**Agent**: parent Grep/Glob/Read (plus `wc`, `awk` and `git show` for measurements; nothing built, nothing run)
**Scope**: `packages/tddy-code-restructuring` `src/plan.rs`, `src/plan/codec.rs`, `src/item_anchor.rs`; the callers of
what would move; the engine operations that can move it; how the budget is measured.

### Sequence

1. Read the approved brief (`stack-brief.md`), the whole-work discovery, `planning-phase.md` Steps 4-5,
   `initial-discovery.md`, `planning-cross-check.md`, `changeset-doc.mdc`, `prd-doc.mdc`.
2. `git show feature/carve/lifecycle-ports-agents:docs/dev/1-WIP/2026-09-26-carve-lifecycle-ports-agents.md` (first 360 lines) —
   the exemplar changeset's discipline (State A/B, Dependencies table, acceptance graph, honest checkboxes).
3. `git rev-parse --short HEAD` / `git status --short` in `.worktrees/engine-fixes-plan` — `a77bca29`, clean except the whole-work discovery file.
4. `wc -l src/plan.rs src/plan/codec.rs src/item_anchor.rs src/plan/codec/*.rs` and `grep -n '#\[cfg(test)\]'` on each —
   1679 / 514 / 979 / 131 lines; test modules open at `plan.rs:521` and `item_anchor.rs:518`, none in `codec.rs`.
5. Read `src/plan/codec.rs` (all 514 lines), `src/item_anchor.rs` (lines 1-530), `src/plan.rs` (lines 1-560).
6. Read `src/runner/budget.rs` (lines 1-140: `files_named_by`, `production_lines`, `budget_report`) and
   `src/runner/entry_points/check_entry_points.rs` (lines 100-356: `check_plan`, where the budget lines go to `options.account`).
7. Grep `rfc3339|hint_of|refuse_split_groups|header_version` and `plan::codec|codec::` in `src` and `tests`; grep
   `owning_package|repo_root_hint|collect_package_files` in `src`, `tests`, `tddy-tools/src`, `tddy-index-daemon/src`; grep
   `Reexport`/`repoints_callers`/`OrderKey`/`is_false` outside `plan.rs`; `grep -l tddy-code-restructuring packages/*/Cargo.toml`;
   grep `RefactorKind|tddy_code_restructuring::plan|item_anchor::` outside the package.
8. Read `docs/same-crate-moves.md`, `src/backends/rust/item_move/outline.rs` (lines 1-140), `tests/impl_item_anchor_acceptance.rs`
   (lines 1-60), `tests/same_crate/mod.rs` (lines 1-60), `.agents/skills/code-restructuring/SKILL.md`,
   `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` and `dead-code-plan-filehint-modified.md`,
   `README.md:105-140`, `docs/dev/todo/2026-10-05-restructure-engine-files-past-the-500-line-budget.md`.
9. Grep `plan/codec|plan\.rs|item_anchor\.rs|owning_package|Reexport` in the package README, `docs/*.md`,
   `docs/ft/coder/rust-code-restructuring.md` and the skill — which documents name the files that change.
10. `awk` over every `src/**/*.rs`, applying the `production_lines` rule, printing files over 480 — and a second `awk` on `runner/tidy.rs`.
11. Read the heads of `2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md`,
    `2026-10-04-restructure-move-item-copies-the-whole-use-header.md`, `2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md`,
    `2026-09-24-restructure-apply-leaves-the-lint-gate-red.md`; `plan-schema.md` lines 305-335 (`to_file`).
12. Read `.config/nextest.toml` and `.config/rust-e2e.filterset` — nothing in this node needs a new test binary.

### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `rfc3339\|hint_of\|refuse_split_groups\|header_version` | `src`, `tests` minus `codec.rs` | `plan.rs:502` (`pub(crate) use codec::hint_of`), `plan.rs:524,1522,1529` (`rfc3339` in tests), `plan_store/live.rs:19,144` (`hint_of`) |
| Grep | `owning_package` | `src`, `tests`, `tddy-tools`, `tddy-index-daemon` | `item_anchor.rs:45`, `item_move/destination.rs:12,26` only |
| Grep | `Reexport` | `src`, `tests` minus `plan.rs` | 32 files (18 `src`, 14 `tests`); `lib.rs:36` re-exports it |
| Grep | `tddy-code-restructuring` | `packages/*/Cargo.toml` | `tddy-daemon-rpc`, `tddy-index-daemon`, `tddy-tools` |
| Awk | production lines (budget rule) > 480 | `src/**/*.rs` | `backends/rust.rs` 2852, `crate_move/test_binary.rs` 967, `plan.rs` 520, `item_anchor.rs` 517, `plan/codec.rs` 514, `backends/rust/item_path.rs` 490 |
| Awk | `#[cfg(test)]` followed by | `runner/tidy.rs` | line 37 is `#[cfg(test)] mod wide_facade_tests;`, so 36 production lines by the rule (README says over budget) |

### Inspected files

#### `packages/tddy-code-restructuring/src/plan/codec.rs`

**Why**: the first seam is `hint_of` + `rfc3339`; the second is `refuse_split_groups`.
**Excerpt**:

```rust
// 212-221
/// What a v2 header says about the file at `path` as it stands: its hash and when it last changed.
pub(crate) fn hint_of(path: &std::path::Path) -> Result<FileHint> { … rfc3339(modified) … }
// 223-253   pub(crate) fn rfc3339(time: std::time::SystemTime) -> Option<String>   (Howard Hinnant's civil_from_days)
// 271-293
fn refuse_split_groups(ops: &[RefactorOp]) -> Result<()> { … }        // called once, from parse_ops (:267)
// 494    mod signature_fields;        <- the precedent: a child module, one call line in parse_op (:489)
```

#### `packages/tddy-code-restructuring/src/item_anchor.rs`

**Why**: the third file; `owning_package` + its two private helpers are a contiguous run.
**Excerpt**:

```rust
// 76-93
/// The directory (relative to `root`) and `[package] name` of the nearest package that holds `file`.
pub(crate) fn owning_package(root: &Path, file: &str) -> Result<(PathBuf, String)> { … repo_root_hint(root, file) … }
// 95-116 fn repo_root_hint(...)      118-137 fn collect_package_files(...)       508 fn malformed(...)  (private; the moved body calls it)
// 518  #[cfg(test)] mod tests
```

#### `packages/tddy-code-restructuring/src/plan.rs`

**Why**: the kind enum, its predicates and `Reexport` are the growth the todo names.
**Excerpt**:

```rust
// 122-124   mod item_path;  pub(crate) use item_path::split_path;  pub use item_path::{Fingerprint, ItemPath, ItemSegment};   <- the facade precedent
// 136-283   pub enum RefactorKind { … }          285-324  impl RefactorKind { edits_a_call_site, moves_across_crates }
// 326-351   pub enum Reexport { Glob, Named, Outside, None }       353-358  impl Reexport { pub(crate) fn repoints_callers }
// 381       pub struct RefactorOp { … }  (#[serde(deny_unknown_fields)])        501-502  mod codec; pub(crate) use codec::hint_of;
// 521       #[cfg(test)] mod tests  (1,159 lines)       524  use crate::{plan::codec::rfc3339, RestructureError};
```

#### `packages/tddy-code-restructuring/src/runner/budget.rs`

**Why**: the instrument that closes this node.
**Excerpt**:

```rust
pub(super) fn files_named_by(plan: &Plan) -> Vec<String>   // "Every file a plan operates on ... the set the budget is reported over"
pub(crate) fn production_lines(text: &str) -> usize        // cut at the first #[cfg(test)] whose next non-blank line starts `mod `/`pub mod `
// "budget: every file the plan names is within {budget} production lines"   /   "budget: {path} is {n} production lines, {k} over"
// a file AT the budget is within it
```

#### `packages/tddy-code-restructuring/src/backends/rust/item_move/outline.rs`

**Why**: whether an `impl` block can travel with its type under `move_item`.
**Excerpt**:

```rust
/// Refused rather than approximated: a range inside an `impl` (its members are reached through
/// their type, and half an `impl` is not an item), a range that cuts an item in half, and a module declaration
pub(super) struct Run { …  /// The named items. An `impl` block is part of the lines and has no name to list.
"…a member of an `impl` cannot move alone … Move the `impl` block itself, with its type"
```

### Findings

- The three numbers in the todo are exact (520 / 514 / 517) and each seam it names is one contiguous run.
- A facade leaves every consumer untouched; the repository already uses that shape in `plan.rs:122-124`.
- `move_item` with `name` is the byte-faithful way to make a child module of the file's own module;
  `extract_module` has two open defect records that matter for a many-seam plan and for comments.
- Moving `RefactorKind` whole buys 190 lines; the two narrower readings leave `plan.rs` within 11 to 51 lines of
  the budget before wave 2 adds four kinds.
- `check --budget` measures a plan's named files, so the closing measurement is a throwaway plan.
- README and three package docs name the old layout; they are wrap-time corrections.

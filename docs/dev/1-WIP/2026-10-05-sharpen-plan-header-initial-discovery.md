# Initial Discovery: `restructure snapshot` writes the header a plan is missing (#sharpen 5/8)

**Changeset**: [2026-10-05-sharpen-plan-header.md](./2026-10-05-sharpen-plan-header.md)
**Date**: 2026-10-05
**Passes**: 2 (Exploration 1 is the whole-work discovery, copied in full; Exploration 2 is this node's own pass)
**Tree**: worktree `.worktrees/engine-fixes-plan` = `origin/master` `a77bca29`. Read-only analysis; nothing was run (no build, no test).

## Combined Conclusions

Narrowed to this node (deficiency **A** of the whole work). The whole-work conclusions are in Exploration 1
and are not repeated.

1. **The claim holds on master, at the lines the stack brief names.** `Plan::parse` refuses a first line that
   is not a header at `packages/tddy-code-restructuring/src/plan/codec.rs:65`
   (`malformed("first line must be a snapshot header")`); `snapshot` is `runner/entry_points/check_entry_points.rs:71`
   and parses with `Plan::parse` at `:74`; `snapshot_resolving` is `:109` and calls `read_plan` (which is
   `Plan::parse`, `runner/entry_points.rs:187`) before it decides anything, at `:116`.
2. **`check` has no header-building path to fix.** The brief reads "the `check`/`snapshot_resolving` paths build a
   v2 header". `check` (`:172`) is `read_plan` then `plan.verify_snapshot`; it never writes a file and never
   built a header. Only `snapshot` (and `snapshot_resolving` through it) writes one. What `check` needs for a
   headerless plan is a refusal that names the remedy, not a header.
3. **Routing already sends a headerless plan to the in-process `snapshot`, and that must keep being true.**
   `item_anchor::plan_file_has_item_anchors` (`item_anchor.rs:249`) swallows the parse error (`Plan::parse(..).ok()`),
   so for a headerless plan it answers `false`, and then: the CLI does not start a language server
   (`restructure_cli.rs:279` `needs_lsp_client`), does not dial a named daemon (`tddy-tools/src/index_client.rs:56-60`
   `answered_without_an_index`), and the daemon does not acquire a server for it
   (`tddy-index-daemon/src/queries.rs:150`). **Consequence for the design: `Plan::parse` must stay strict.**
   A tolerant `Plan::parse` would make that function answer `true` for a headerless plan of item anchors (which
   is nearly every one) and route the first `snapshot` through a cold rust-analyzer. The tolerant read is a
   separate function used only by `snapshot`.
4. **Nothing in routing, proto or `tddy-tools` needs an edit.** One line in `snapshot_resolving` does: `read_plan(&path)?`
   fails first for a headerless plan, so the daemon's `Snapshot` RPC and the CLI's cold path would still refuse. It is
   replaced by `plan_file_has_item_anchors(&path)`, which is the same question asked without the failure.
5. **`restructure anchors` takes a file, not a plan.** `anchors <file.rs> --items A,B` / `--at L:C-L:C`
   (`restructure_args.rs`; SKILL.md CLI block) emits one anchor. It reads no plan, so a headerless plan changes
   nothing about it. The code issue `broken-restructure-anchors-empty-outline.md` is the reason not to route this
   change through it.
6. **The header computation has one precedent writer to reuse.** `plan/codec.rs:213` `hint_of` (the whole-work file says `:219`; the line is `:213`) writes a v2 hint
   (`sha256` + `modified`); `rehashed_header` (`:90`) already serialises `HintedHeader`. A headerless plan's header is
   the same value, over the set of files the operations anchor instead of the set the old header named. Using
   `hint_of` honours the open code issue `dead-code-plan-filehint-modified.md` (no second writer of `modified`).
7. **A v2 header is a weaker promise than a v1 header for coordinate anchors (a gap in the brief).** v2 "never
   refuses a run" (`plan-schema.md`, line 17-19) because an item anchor does not depend on the rest of the file.
   A `range` or `symbol` anchor does. A headerless plan the engine headers as v2 would run range anchors
   against a drifted file with only a progress line. This is an OPEN decision in the changeset (recommendation:
   v1 when any anchor is a coordinate).
8. **File budget.** `plan/codec.rs` is 514 production lines (no `#[cfg(test)]` module), `plan.rs` 520, `item_anchor.rs` 517.
   `tidy-engine-files` (K=1) brings all three to <= 500. This node must put its code in a new child module
   (`plan/codec/headerless.rs`), the `signature_fields.rs` precedent, and edit `codec.rs` by one `mod` line and one
   message call. It does not touch `plan.rs` or `item_anchor.rs`.
9. **Test levels.** Library level (milliseconds, no server): `tests/snapshot_rewrites_the_header.rs` is the template
   and `runner::snapshot` / `runner::snapshot_resolving` are public. Daemon level with fake language servers:
   `tddy-index-daemon/tests/code_index_service_acceptance.rs:1722`
   `snapshots_a_plan_with_no_item_anchors_without_waiting_for_a_language_server` is the template. CLI child process:
   `tddy-tools/tests/restructure_cli_acceptance.rs` (`assert_cmd`). No live rust-analyzer test is needed, so no
   `.config/rust-e2e.filterset` or `.config/nextest.toml` entry.
10. **Not verified (nothing was run):** that `Fingerprint` accepts a syntactically valid but unresolved value (the
    daemon test below uses a symbol anchor to avoid depending on it); that `FileHint.modified` is byte-stable
    between two runs on an unchanged file on every platform (it is the file's mtime to the second, so it is, but
    no run proved it).

## Exploration 1: the whole-work discovery, copied in full — 2026-10-05

**Agent**: parent Grep/Glob/Read (the whole-work planning pass)
**Scope**: every deficiency of the restructure-engine fixes (A, B1-B3, C1, C2a, C2b, D1, D2). Copied **verbatim** from
`docs/dev/1-WIP/2026-10-05-engine-fixes-whole-work-initial-discovery.md` on the planning branch, with its headings demoted two
levels so the nested `Combined Conclusions` and `Exploration 1-3` of the original do not collide with this file's own.
Where this node's own passes found the whole-work text wrong, the correction is in this file's Combined Conclusions.

### Initial Discovery: restructure engine fixes (whole work, PR stack)

**Changeset**: not yet written — planned slug `2026-10-05-engine-fixes-whole-work` (this file is the whole-work
companion; each stack node gets its own companion later, per `initial-discovery.md`)
**Date**: 2026-10-05
**Passes**: 3
**Tree**: worktree `.worktrees/engine-fixes-plan`, branch `engine-fixes-planning` = `origin/master` `a77bca29`. Read-only analysis.

#### Combined Conclusions

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

#### Exploration 1: the deficiency records and the deferred-work scan — 2026-10-05

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

#### Exploration 2: the engine — snapshot, move_item, facades, op registration — 2026-10-05

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

#### Exploration 3: the wait, the spawns, and the test machinery — 2026-10-05

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

## Exploration 2: snapshot, the header codec, routing and test style, re-read for this node — 2026-10-05

**Agent**: parent Read/Grep (a subagent of the planning stack, read-only)
**Scope**: every place a headerless plan is read, and every place a header is written, on `a77bca29`; the
three test files this node's tests are modelled on.

### Sequence

1. Read the stack brief and the whole-work discovery (copied as Exploration 1).
2. Read `packages/tddy-code-restructuring/src/plan/codec.rs:1-260` (`Plan::parse`, `rehashed_header`, `drifted_hints`,
   `to_jsonl`, `hint_of`, `rfc3339`, `header_version`) and `:270-500` (`parse_op` chain).
3. Read `runner/entry_points/check_entry_points.rs:1-360` (`snapshot`, `snapshot_resolving`, `status`, `check`,
   `check_plan`) and `runner/entry_points.rs:60-70, 181-195` (the dispatch and `read_plan`).
4. Read `item_anchor.rs:235-262` (`has_item_anchors`, `plan_file_has_item_anchors`, `lower`).
5. Read `restructure_cli.rs:255-300` (`needs_lsp_client`, `names_item_anchors`), `tddy-tools/src/index_client.rs:30-90`
   (`run_restructure`, `answered_without_an_index`), `tddy-index-daemon/src/queries.rs:100-190`
   (`serve_snapshot`, `snapshotted`).
6. Grep `Plan::parse(` over `packages` (callers: `plan_store.rs:348`, `item_anchor.rs:252`, `tddy-daemon-rpc/src/code_navigation/plan.rs:51`,
   `check_entry_points.rs:74`, `runner/entry_points.rs:188`) and `first line must be a snapshot header` over `packages` and `docs`.
7. Read `plan.rs:60-135` (`Anchor::file`, `FileHint`) and `:485-515` (`RefactorOp::anchors`, `Plan`, `malformed`).
8. Read `tests/snapshot_rewrites_the_header.rs` (whole), `tddy-tools/tests/restructure_cli_acceptance.rs:1-110`,
   `tddy-index-daemon/tests/code_index_service_acceptance.rs:1660-1780`, `tests/harness/mod.rs:223-260` (`a_plan_of`, `a_hinted_plan_of`).
9. Read the todo on `feature/carve/lifecycle-ports-agents`:
   `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md`.
10. Read `packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md` and the engine-files todo.

### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `first line must be a snapshot header` | `packages`, `docs` | `plan/codec.rs:65` only in source; no test pins the text |
| Grep | `Plan::parse(` | `packages` (non-test) | `plan_store.rs:348`, `item_anchor.rs:252`, `check_entry_points.rs:74`, `entry_points.rs:188`, `tddy-daemon-rpc/.../plan.rs:51` |
| Grep | `plan_file_has_item_anchors` | `packages` | `item_anchor.rs:249`, `restructure_cli.rs:295`, `index_client.rs:60`, `queries.rs:150` |
| Grep | `pub fn rehashed_header\|fn hint_of` | `plan/codec.rs` | `:90`, `:213` |
| Grep | `Snapshot` | `tddy-index-daemon/tests`, `tddy-tools/tests` | `code_index_service_acceptance.rs:1665-1760`, `index_daemon_client_acceptance.rs` |

### Inspected files

#### `packages/tddy-code-restructuring/src/plan/codec.rs`

**Why**: where the header is required and where a header is built.
**Excerpt**:

```rust
// Plan::parse, :55-66
let header = lines.next().ok_or_else(|| malformed("plan is empty"))?;
if header_version(header) == Some(HINTED_SCHEMA_VERSION) { ... }          // a v2 header
let header: SnapshotHeader = serde_json::from_str(header)
    .map_err(|_| malformed("first line must be a snapshot header"))?;     // :65 — a headerless plan ends here
// rehashed_header, :90 — "Only the paths the header already names"
for path in self.files.keys() { files.insert(path.clone(), hint_of(&root.join(path))?); }
// hint_of, :213
let modified = std::fs::metadata(path).and_then(|m| m.modified())
    .map_err(|error| malformed(format!("{} could not be read: {error}", path.display())))?;
```

A line-1 operation has no `v` key, so `header_version` is `None`, the `SnapshotHeader` deserialisation fails, and the
refusal says nothing about the operation or the remedy. `hint_of` fails with `plan is malformed: <path> could not be
read: …` for a file that is missing, which is already the right class and wording for an anchored file that is not there.

#### `packages/tddy-code-restructuring/src/runner/entry_points/check_entry_points.rs`

**Why**: `snapshot` is the only writer of a header; `snapshot_resolving` is the router.
**Excerpt**:

```rust
pub fn snapshot(root: &Path, options: Options) -> Result<SnapshotRewrite> {        // :71
    let path = options.plan()?;
    let text = std::fs::read_to_string(&path)?;
    let plan = Plan::parse(&text)?;                                                // :74 — fails headerless
    let header = plan.rehashed_header(root)?;
    let blank: usize = text.split_inclusive('\n').take_while(|l| l.trim().is_empty()).map(str::len).sum();
    let produced = match text[blank..].split_once('\n') {                          // replaces the first non-blank line
        Some((_, operations)) => format!("{}{header}\n{operations}", &text[..blank]),
        None => format!("{}{header}", &text[..blank]),
    };
    ...  paths: plan.snapshot.len() + plan.files.len(), rewritten, stale: Vec::new()
pub fn snapshot_resolving(...) {                                                   // :109
    if !has_item_anchors(&read_plan(&path)?) { return snapshot(root, options); }   // :116 — read_plan fails first
```

The splice **replaces** line 1. For a headerless plan line 1 is an operation, so a header must be **inserted** before
it and the operation bytes left alone. `check` (`:172`) is `read_plan` + `verify_snapshot` and writes nothing.

#### `packages/tddy-code-restructuring/src/item_anchor.rs`, `restructure_cli.rs`, `tddy-tools/src/index_client.rs`, `tddy-index-daemon/src/queries.rs`

**Why**: routing for a plan that does not parse.
**Excerpt**:

```rust
pub fn plan_file_has_item_anchors(path: &Path) -> bool {                           // item_anchor.rs:249
    std::fs::read_to_string(path).ok().and_then(|text| Plan::parse(&text).ok()).is_some_and(|plan| has_item_anchors(&plan))
}
Command::Snapshot => names_item_anchors(options),                                  // restructure_cli.rs:279
RestructureCommand::Snapshot(snapshot) => !plan_file_has_item_anchors(&snapshot.plan)   // index_client.rs:59-61
let client = if item_anchor::plan_file_has_item_anchors(&plan) { Some(index.client_for(&root).await?) } else { None };  // queries.rs:150
```

All four swallow the parse error and route a headerless plan to the in-process `snapshot`. They are consistent today by
accident of `.ok()`; this node makes it a tested property.

#### `packages/tddy-code-restructuring/tests/snapshot_rewrites_the_header.rs`

**Why**: the library-level style. `a_workspace_holding(source)`, `a_plan_pinning(root, digest, ops)`, `a_snapshot_of(plan)`
over `runner::snapshot(root, Options{command: Command::Snapshot, target: Some(plan), ..})`; assertions on the first
line and on `operations == vec![AN_EXTRACTION, A_RENAME]` (byte identity). Temp git-less workspace with `src/lib.rs`.

#### `packages/tddy-tools/tests/restructure_cli_acceptance.rs`

**Why**: the CLI style. `cargo_bin_cmd!("tddy-tools")` with `env_remove("TDDY_SOCKET")`, `current_dir(tempdir)`,
`args(["restructure", "<sub>", plan])`, `assert().failure()` and a `stderr` contains check. No rust-analyzer. The
existing `restructure_check_rejects_malformed_plan_header` uses an **empty** file (`plan is empty`), so it does not
pin the text this node changes.

#### `packages/tddy-index-daemon/tests/code_index_service_acceptance.rs:1722`

**Why**: the daemon style. `snapshots_a_plan_with_no_item_anchors_without_waiting_for_a_language_server` writes a plan to a
temp package, calls the registered `Snapshot` coordinate over `a_host_over_fake_language_servers()`, asserts
`(answer.paths, answer.rewritten, answer.stale) == (1, true, vec![])` and that `Workspaces` lists no held root.

#### `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md` (on `feature/carve/lifecycle-ports-agents`)

**Excerpt**: "`restructure snapshot plan.jsonl`, the command the skill names for 'rewrite the header', refuses the same file
with the same text, because it only rewrites a header that is already there." and "A v2 header is only hints for item
anchors, so the engine can compute it from the plan's own anchors' `file` fields."

### Findings

- The only production edits are: a tolerant read used by `snapshot` alone, a header built from the anchored files, an
  insert (not replace) splice, one line in `snapshot_resolving`, and a refusal message that names `restructure snapshot`.
- `Plan::parse` stays strict; routing depends on it.
- `check`, `apply`, `status`, `load` and the code-navigation plan reader keep refusing a headerless plan.
- No test pins the current refusal text, so the improved text breaks nothing. The refusal class `plan is malformed:` is the
  `thiserror` display of `RestructureError::MalformedPlan` (`src/lib.rs:45`), not a string match, so a message that keeps
  that variant keeps its class.
- v2 for coordinate anchors loses drift protection: an OPEN decision.

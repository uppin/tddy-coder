# Initial Discovery: repoint-call (#sharpen 7/8)

**Changeset**: [2026-10-05-sharpen-repoint-call.md](./2026-10-05-sharpen-repoint-call.md)
**Date**: 2026-10-05
**Passes**: 2
**Tree**: worktree `.worktrees/engine-fixes-plan`, branch `engine-fixes-planning` = `origin/master` `a77bca29`. Read-only analysis.

## Combined Conclusions

Narrowed to `repoint-call` (`#sharpen` 7/8, C2a). The whole-work conclusions (Exploration 1) hold; Exploration 2
re-read the code behind them for this node and **corrected five claims** (items 1, 2, 4, 9 and 11 below are the corrections).

1. **There is no `callee` machinery to extend; there is call-*argument* machinery to reuse.**
   `backends/rust/signature_rewrites/call_site.rs` (175 lines) reads one call out of an item-relative range
   (`call_in`, `:99`): `syn::parse_str::<Expr>` must give `Expr::Call` or `Expr::MethodCall`, the `(` is found on the masked
   text (`opening_of_the_last_group`), and the argument count `syn` reports must equal the count the scan reads
   (`server_defect` otherwise). It writes **only argument spans**; nothing in it knows the span in front of the `(`.
   `call_in`, `Call`, and the helpers in `signature_rewrites.rs` (`Span`, `Replacement`, `edits_of`) are private to
   those two files, so the new operation needs **visibility-only** widening (`pub(in crate::backends::rust)`) or a copy;
   the recommendation is the widening.
2. **Correction: `sites_of` is at `item_move.rs:141`, not `:131`.** It is `pub(super)` on `RustBackend`, takes
   `named: &[(&str, &Value)]` (a name and the outline's LSP position of the item) and returns `Site { path, offset, name }`,
   one per **reference to the name token**, with `includeDeclaration: false` (`rust.rs:1666 references_at`). Its position
   argument can be built from a lowered item anchor: an `item` anchor with neither `start` nor `end` lowers to a **zero-width
   range at the item's name** (`item_anchor.rs:166-173`, `range_within`), which is exactly the position
   `textDocument/references` needs. So a bulk form needs no outline walk of its own, and the move-item outline code
   (`item_move/outline.rs`, which **refuses a range inside an `impl`**) is not on its path.
3. **A site is a name token, not a call.** Everything after the offset is this node's work: read `(` after the name (skipping
   a turbofish), read `.` before it, and walk **backwards** over the receiver's postfix chain. That walk is the only new
   text-reading algorithm in the node; it is validated by handing the resulting range to `call_in`, so "exactly one call" is
   decided by `syn`, not by the scan. Doc-link and comment positions are blank in `masked_to_code`
   (`early_return.rs:271`), so a reference there is *seen* (if rust-analyzer reports it: the open B3 probe) and *skipped*.
4. **Correction: the op slice is wider than "two fields".** `RefactorOp` has 14 fields and `#[serde(deny_unknown_fields)]`
   (`plan.rs:381-447`); it does not derive `Default`, so **every struct literal** of it lists every field. `grep -rn
   'order: Vec::new()' packages/tddy-code-restructuring/{src,tests}` finds **20 full literals in 15 files** (5 in `src/`,
   15 in `tests/`; `tddy-discovery` has an unrelated `order` field). Each new field is
   one line in each: a compile error (`E0063`) in every test target, which `cargo build -p` does not see (use `cargo check
   --all-targets`; memory `cargo-build-p-misses-test-targets`). `retarget-impl` (K=6) adds its own field the same way,
   so two nodes will conflict on the same 20 adjacent lines. **One** new field (`callee`) for this node halves the cost; the
   whole-work discovery assumed two (`callee`/`receiver`). Re-measured on `a77bca29` for the 2026-10-05 decision (each node pays for its own field in
   its own first commit): `git grep -n 'RefactorOp {' -- packages | wc -l` is 80 textual sites, of which 2 are definitions, 41 function signatures,
   17 `..base` struct updates (no edit) and 20 full literals, in 15 files; the `order: Vec::new()` grep gives the same 20 in 15 files.
5. **Where the op lives in the slice** (same shape as `cb50ab5c`/#584): `RefactorKind::RepointCall` in `plan/refactor_kind.rs` (on `a77bca29`
   it is `plan.rs:147-283`; `tidy-engine-files` D1, decided 2026-10-05, moves the kind there) and the `callee` field in `plan.rs` (`RefactorOp`, `:376-448`, which stays), a codec child module (precedent `plan/codec/signature_fields.rs`, 131 lines; `parse_op`
   is `codec.rs:295-493` and one call longer), `plan/rust_syntax.rs` (`one_expr` at `:35` is the precedent for a callee
   parser), `backends/rust.rs` (`SUPPORTED` `:66` array length 22 -> +1, `check` arm `:1011`, `resolve` arm `:1168`) and a new
   `backends/rust/repoint_call/` module. `tddy-tools` names no operation (whole-work finding, re-confirmed:
   `grep -rn "ReorderCallArgs"` finds the engine only), so nothing outside `tddy-code-restructuring` and docs changes.
6. **`restructure verify` takes no plan and no op list**: `verify::compare(before, after)` is a statement-multiset
   comparison with five passes (`verify.rs:1-50`). Pass 4 deletes lowercase `segment::` qualifiers; pass 5 compares token
   multisets. A receiver gaining a hop (`self.f(x)` -> `self.peer.f(x)`) adds the tokens `.` and `peer`: **no pass pairs it**, so
   `verify` reports it as one statement lost and one gained, per call site. The teaching therefore has to be a **new pass**
   inside `compare`, not a plan-reading mode.
7. **`Excused` is on the wire.** `verify::Excused { repointed, visibility, cfg_test_gates }` is mapped into the index
   daemon's `VerifyResponse` (`tddy-index-daemon/src/queries.rs:305-307`) and rendered by `tddy-tools/src/index_console.rs`
   and `tddy-index-daemon/src/render.rs`. A new counter field is a proto and three-package change; counting the new pairs
   under `repointed` is not. (Reflow pairs are already counted there: `verify.rs:46`, "there is deliberately no separate
   wire count".)
8. **Test levels.** The pure text function and the plan codec are testable in milliseconds with no server: inline unit tests
   (as `signature_rewrites.rs` has) and a library-level binary over `Plan::parse` and a static `runner::check`
   (`tests/library_returns_its_results.rs` and `tests/snapshot_rewrites_the_header.rs` are the style; a static check judges
   **range** anchors only, `check_entry_points.rs`: "unresolvable_without_a_server"). Anything that lowers an item anchor
   or asks for references needs rust-analyzer: a live binary in the style of `signature_rewrites_acceptance.rs` /
   `move_item_acceptance.rs`, with `cargo check` as the assertion. A **new live binary must be registered in two places**:
   `.config/rust-e2e.filterset` and the `rust-analyzer` test-group override in `.config/nextest.toml:88-103`
   (correction: the rule that says so is the `nextest.toml` header, `:18-25`, not the filterset file, which has no header).
9. **Correction: `rust.rs` wiring for a text-only form.** The single form is answered before any server exists (like
   `rewrite_signature`, `rust.rs:1210`); only the bulk form starts one. The resolve arm must therefore sit in the block
   above `self.start(...)`, and start the server itself for the bulk form (as `move_items` does, `item_move.rs:71`).
10. **`check --deep` does not print `Resolution.notes`.** `Rehearsal::rehearse` (`runner/rehearsal.rs`) records the edit and
    drops `resolved.notes` and `resolved.report`; only `apply` prints them (`runner/entry_points.rs:203-210`). A site list for
    this node's bulk form in `check --deep` would need that plumbing, which is `repoint-facade`'s deliverable (its acceptance
    check A4 needs it). **This node does not build it**, so there is no edge from `repoint-call` to `repoint-facade` and the
    stack's edge list is unchanged.
11. **Correction: `signature_rewrites_acceptance.rs` is a live binary but is not in `.config/rust-e2e.filterset`**, nor are
    the `move_item_*`/`reparent_module_*` binaries (the whole-work discovery noted the latter). Existing gap, not ours.
12. **No Step 2b claim.** `grep -rl 'Claimed by:'` over the three packages finds one file, whose value is `none`.
    Master todos in the path: the `state-parameter` todo stays open on purpose (developer decision); the file-budget todos are
    ⚠ During. Open items to settle before green are the changeset's O1-O10.
13. **Update from the sibling drafts (after Exploration 2).** `retarget-impl` (K=6) builds the `verify` declaration carrier
    (`verify::compare_with(.., &Declared)`, repeatable `--retarget OLD=NEW`, `VerifyRequest.retargets = 3`, rules R1/R2 counted in `repointed`).
    This node therefore teaches `verify` by **extending that carrier** (`--repoint OLD=NEW`, rule R-call), not by an inferred pass: a real edge by surface
    from `retarget-impl`, in addition to file overlap with `tidy-engine-files`. `plan-header` adds `plan/codec/headerless.rs` beside this node's
    `plan/codec/repoint_call_fields.rs`. Items 6 and 7 above (why a new pass inside `compare` and why no wire counter) still hold.

## Exploration 1: the whole-work discovery of the engine fixes, copied in full — 2026-10-05

**Agent**: parent Grep/Glob/Read (earlier session)
**Scope**: the whole `#sharpen` work (all eight nodes). Copied verbatim from
`docs/dev/1-WIP/2026-10-05-engine-fixes-whole-work-initial-discovery.md`, headings demoted one level so
that its own `Combined Conclusions` and three explorations sit inside this pass. Its conclusions are the
**whole-work** reading; this node's reading is Combined Conclusions above.

## Initial Discovery: restructure engine fixes (whole work, PR stack)

**Changeset**: not yet written — planned slug `2026-10-05-engine-fixes-whole-work` (this file is the whole-work
companion; each stack node gets its own companion later, per `initial-discovery.md`)
**Date**: 2026-10-05
**Passes**: 3
**Tree**: worktree `.worktrees/engine-fixes-plan`, branch `engine-fixes-planning` = `origin/master` `a77bca29`. Read-only analysis.

### Combined Conclusions

#### 0. Headline

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

#### 1. Per deficiency: where it lives, smallest change, claim verified?

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

#### 2. Shared machinery and real edges

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

#### 3. Vertical slice of an operation, and test level

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

#### 4. Step 2b — both records, every package in scope

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

#### 5. Existing todos these nodes could resolve

See table above: the two `no deadline` todos (conditional on D1's bound), the `state parameter` todo (conditional on a
generalised C2a), and the nine brief todos once on master. Nothing else on master is closed by A, B or C1.

#### 6. Proposed edges and waves (linear stack, green in waves)

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

#### 7. Decisions the developer must make before the changeset

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

### Exploration 1: the deficiency records and the deferred-work scan — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: the nine #532/#586 todos, both Step 2b records for the packages in scope, open PRs.

#### Sequence

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

#### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `Claimed by` | `packages/*/docs/code-issues` (four packages) | only `broken-restructure-anchors-empty-outline.md:6` → value is `none` |
| Grep | `restructure\|…` | `docs/dev/todo` | 69 files; classified in Combined Conclusions §4 |
| Bash | `gh pr list --state open` | repo | #586 touches `run-index-daemon` + `SKILL.md`; #532 touches only todos |

#### Inspected files

##### `docs/dev/todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md` (on #532)

**Why**: claim D1.
**Excerpt**:

```
restructure check <plan> --deep      -> "no findings"            (finished)
restructure apply <plan>             -> did not return
indexing (+0ms): waiting for type inference at the anchor
indexing (+2.0s): working: build script num-bigint run
… "I did not investigate the cause."  / "Not reduced."
```

##### `docs/dev/todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md` (on #586)

**Why**: claim D2. States CrowdStrike Falcon "Malicious script was blocked" in the same window; asks for (1) a spawn log of every process
the engine, CLI and daemon start, with exit status or signal; (2) the daemon's own exit recorded by a parent that is not the pid `--stop`
signals; (3) a per-operation line naming the op id. "the engine change is its own node".

##### `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md` (on #532)

**Excerpt**: `Error: the index daemon refused this run (InvalidArgument): plan is malformed: first line must be a snapshot header`;
"A v2 header is only hints for item anchors, so the engine can compute it from the plan's own anchors' `file` fields."

##### the other six #532 todos

`facade-import` (B2/C1: `crate::config` is `pub use tddy_daemon_kernel::config`; 12 files, 30 lines, 4 paths in grouped `use`),
`move-item-copies-a-moved-signature-s-facade-path` (`agent_roster.rs:186` holds `crate::config::DaemonConfig` after a move),
`…-intra-doc-links` (`handler_state.rs:106`), `…-writes-a-caller-re-point-as-a-full-path`
(`crate::connection_service::agent_roster::split_forward_deadline(&self.config)`), `no-operation-re-points-a-calls-receiver-or-writes-a-delegator`
(28 call sites + 7 wrappers by hand), `no-operation-retargets-an-impl-to-another-type` (22 methods; `change_param_type` on `self` refused:
`this seam cannot be cut here: self is not a parameter of the function the anchor names`).

##### `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md`

**Why**: the standing record of the waiting design.
**Excerpt**:

```
## If you are about to change this code
Never wrap a `restructure` call in `timeout` and never pipe it. The tool's waiting behaviour is the
design — a run waits until the server is ready or until you stop it — and both a timeout and a
pipeline destroy that signal.
```

##### `docs/dev/todo/2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting.md`

**Excerpt**: "A bounded wait would be a budget, which the restructuring library deliberately withdrew in favour of cancellation … It needs the developer to choose."

##### `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md`

2,853 production lines at 2026-10-05; "The impl-member seams remain; none was cut"; each of the last three growth rows is wiring only.

#### Findings

- All nine todos describe real behaviour; none is already fixed on master.
- The 9 todos are not on master; two link a todo deleted by #540.
- No claimed code issue is in the path; the only `Claimed by:` hit is `none`.
- D conflicts with a written design rule and with two deferred todos.

### Exploration 2: the engine — snapshot, move_item, facades, op registration — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: `packages/tddy-code-restructuring`, `packages/tddy-tools`, `packages/tddy-index-daemon`.

#### Sequence

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

#### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `first line must be a snapshot header` | `src` | `plan/codec.rs:65` |
| Grep | `MoveItem` | packages, minus `item_move/`, tests | `plan.rs:231`, `codec.rs:349,363,400`, `rust.rs:77,1011,1168` |
| Grep | `reexports::followed\|followed\b` | `crate_move*` | `crate_move/survey.rs:101` is the only production call |
| Grep | `survey_moved_file(` | `src` | `header.rs:73`, `preconditions.rs:97` |
| Grep | `edits_for_file(` | `src` | `item_move/assemble.rs:308`, `module_reparent/assemble.rs:153` |
| Grep | `intra-doc\|doc link` | item_move, module_reparent, inline_paths, crate_move | none |
| Awk | production lines | 36 files | `rust.rs` 2852, `plan.rs` 520, `codec.rs` 514, `item_anchor.rs` 517, `readiness.rs` 290, `sites.rs` 333, `assemble.rs` 429, `rebase.rs` 191, `header.rs` 389, `survey.rs` 169, `reexports.rs` 259, `journal.rs` 460 |

#### Inspected files

##### `packages/tddy-code-restructuring/src/plan/codec.rs`

**Why**: A — where the header is required; the rule chain every op joins.
**Excerpt**:

```rust
let header: SnapshotHeader = serde_json::from_str(header)
    .map_err(|_| malformed("first line must be a snapshot header"))?;          // codec.rs:65
// parse_op: 295..493, then `mod signature_fields;` (131 lines, the precedent) at 494
if op.op == RefactorKind::MoveItem { names_a_destination_and_anchors_by_item(&op, "move_item", …)?; }   // :363
```

##### `packages/tddy-code-restructuring/src/runner/entry_points/check_entry_points.rs`

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

##### `packages/tddy-code-restructuring/src/backends/rust/item_move/sites.rs`

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

##### `packages/tddy-code-restructuring/src/backends/rust/item_move/rebase.rs`

**Why**: B2 — the only pass over the moved text.
**Excerpt**: `path_edit` (`:99`) starts at `at` and loops `if rest.starts_with("super::") … else if rest.starts_with("self::") … else break; if chain == 0 { return None; }` — a `crate::` head returns `None`.

##### `packages/tddy-code-restructuring/src/crate_move/{survey,reexports,header}.rs`, `docs/path-survey.md`

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

##### `packages/tddy-code-restructuring/src/verify.rs`

**Why**: whether new ops stay accountable.
**Excerpt**: "Re-point pairing: … equal once lowercase module qualifiers are deleted (`f(` becoming `m::f(`)"; "It cannot excuse a renamed callee, a changed or dropped argument…". `use` items are scaffolding, wholly.

##### `packages/tddy-code-restructuring/src/plan.rs`

`RefactorKind` `:147-283` (138 lines), `impl RefactorKind` `:285`, `Reexport` `:334`, `RefactorOp` `:381` with `#[serde(deny_unknown_fields)]`; production 520 before `mod tests` at `:521`.

##### `git show cb50ab5c` (the exemplar slice)

`rust.rs` +24 (module decls, SUPPORTED 20→22, check arms, resolve arms); `plan.rs` +184; `plan/codec.rs` +79; `console.rs` +50;
`item_anchor.rs` +58; `restructure_args.rs`/`restructure_cli.rs`/`runner/entry_points.rs`/`runner/options.rs`/`lib.rs` changes were for
`warm`; 10 acceptance binaries; docs: `SKILL.md` +49, `plan-schema.md` +82, `docs/ft/coder/rust-code-restructuring.md` +116, package README +15, `docs/same-crate-moves.md` (110).

#### Findings

- A: header creation belongs in `snapshot` (library). The daemon and CLI already route correctly.
- B1: 2 callers of `edits_for_file`; fix confined to `sites.rs` (+ `text.rs` helper visibility).
- B2: only `rebase.rs`'s `path_edit` reads the moved text; facade resolution exists in `crate_move` and is private.
- B3: nothing handles doc links; whether RA returns them as references is unknown.
- C1/B2 share a resolver; C2 shares call-site parsing (`signature_rewrites/call_site.rs`) and `sites_of` references.
- New op slice is engine-only plus docs; `tddy-tools` is generic.
- `verify` cannot account for receiver/impl-header rewrites.

### Exploration 3: the wait, the spawns, and the test machinery — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: D1/D2 code, test harness and CI registration.

#### Sequence

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

#### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `waiting for type inference` | engine `src` | `readiness.rs:148` |
| Grep | `self.wait_until_resolved(uri, &position)` | `rust.rs` | `:2004`, `:2049` (rename_symbol), `:2142` (rename_placeholder) |
| Grep | `Command::new\|\.spawn()` | engine, daemon, lsp, tools | `rust.rs:685,706`; `compile_gate.rs:253,259`; `tidy.rs:551,565`; `tidy/format.rs:76`; `apply.rs:39,177,189`; `tddy-lsp/server_body.rs:55,70`; tools `cli.rs:595` (unrelated) |
| Grep | `serverStatus` | `fake_lsp.rs` | `:317` (`quiescent: true` only) |
| Grep | `move_item` in `.config` | | none: `move_item_*`/`reparent_module_*` binaries are not in `rust-e2e.filterset` |

#### Inspected files

##### `packages/tddy-code-restructuring/src/backends/rust/readiness.rs`

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

##### `packages/tddy-index-daemon/src/apply.rs`

The daemon drives its own apply loop (`apply_held_plan`), calling the same backends through `runner::registry_for`; cancellation is checked between operations and inside the waits. A fix in `readiness.rs` therefore serves both the cold and the warm path.

##### Spawn sites

Engine: `rust.rs:685` rust-analyzer; `compile_gate.rs:253` `cargo check` (already killed on cancel, `:273-285`); `tidy.rs:551,565` cargo; `tidy/format.rs:76` rustfmt; `apply.rs:39,177,189,227..` git. Daemon-side rust-analyzer: `tddy-lsp/src/server_body.rs:55` (`register_child_pid`). Daemon process: `run-index-daemon` (`setsid`, nix env capture `nix develop … env -0`). `journal.rs` (460 production lines) records only `WorkspaceEdit`s.

##### Tests

`tests/same_crate/mod.rs`: fixtures are one package `app`, ops built from JSON; `moving_items(…)` helpers; live RA per test. `tests/snapshot_rewrites_the_header.rs`: library-level, temp git workspace, `runner::snapshot`. `tddy-tools/tests/restructure_cli_acceptance.rs`: `assert_cmd` against the real binary, no RA.

#### Findings

- The hang is the unbounded not-ready branch of `await_answer`; cancellation is the only exit by design.
- A fix serves cold and warm paths from one place; the daemon failure path must also leave no poisoned latch.
- Spawn sites are enumerable in six files plus the launcher script; RA's own children are out of reach.
- Test levels: fake LSP for D1, live RA for B/C2b, library for A, CLI for A end to end; a new live binary needs `rust-e2e.filterset`.

## Exploration 2: the call-site machinery, the references path, the op slice and `verify` — 2026-10-05

**Agent**: parent Grep/Glob/Read
**Scope**: `packages/tddy-code-restructuring` (`backends/rust/signature_rewrites*`, `backends/rust/item_move.rs`, `plan.rs`,
`plan/codec*`, `item_anchor.rs`, `verify.rs`, `runner/`), the test harness, `.config/`; the three master/brief todos for C2a.
Read-only; nothing was built or run.

### Sequence

1. Read the stack brief and the whole-work discovery (Exploration 1 here).
2. Read `planning-phase.md` Steps 4-5, `initial-discovery.md`, `planning-cross-check.md`, `changeset-doc.mdc`, `prd-doc.mdc`,
   `code-restructuring/SKILL.md` and `references/plan-schema.md` (operations table and the call-site section).
3. `git show --stat=200 cb50ab5c` (the #584 slice) and `docs/same-crate-moves.md`, `path-survey.md`, `facades.md`.
4. Read `backends/rust/signature_rewrites/call_site.rs` and `signature_rewrites.rs:1-200`.
5. Read `backends/rust.rs:60-100` (`SUPPORTED`), `:990-1060` (`check`), `:1155-1240` (`resolve`), `:1666-1683` (`references_at`),
   `:1890-1940` (`rewrite_signature`).
6. Read `backends/rust/item_move.rs` (all) and `item_move/sites.rs:1-120`.
7. Read `item_anchor.rs:195-330` (`resolve_item_anchors`, `lower`) and `:150-200` (`range_within`).
8. Read `plan.rs:20-140` (`Anchor`), `:140-330` (`RefactorKind`), `:376-450` (`RefactorOp`); `plan/codec.rs:295-514`
   (`parse_op`); `plan/codec/signature_fields.rs`; `plan/rust_syntax.rs`.
9. Read `verify.rs:1-420`, `verify/statements.rs` (`analyse`, `drop_use_gates`, `strip_qualifiers`).
10. Read `runner/rehearsal.rs`, `runner/entry_points.rs:120-215`, `runner/entry_points/check_entry_points.rs:196-356`, `edit.rs:55-90`.
11. Read `tests/signature_rewrites_acceptance.rs` (heads and the not-a-call refusal), `tests/same_crate/mod.rs:1-140`,
    `tests/move_item_acceptance.rs:1-80`, `tests/same_crate_deep_check_acceptance.rs:1-80`, `tests/harness/mod.rs` (`applying_a_plan_of`,
    `checking_the_plan`, `performing`, `resolving`).
12. Read `.config/rust-e2e.filterset` and `.config/nextest.toml:1-130`.
13. Read the master todo `2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md` and, from the
    scratchpad, the #532 todo `2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md`.
14. Counted literals of `RefactorOp`; counted consumers of `Excused`; checked which packages construct `RefactorOp`.

### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `fn sites_of` / `fn rewrite_statement` / `fn requalified` | `backends/rust/item_move*` | `item_move.rs:141`; `sites.rs:247`; `sites.rs:216` (replace at `:235`) |
| Grep | `order: Vec::new()` | `packages/` | 20 full `RefactorOp` literals in the engine (5 `src/`, 15 `tests/`); `tddy-discovery` is an unrelated struct |
| Grep | `ReorderCallArgs\|reorder_call_args` | repo | engine `plan.rs:282,295`, `rust.rs:88`, `call_site.rs:46`, `signature_fields.rs`; docs: `plan-schema.md:137,151`, `docs/ft/coder/rust-code-restructuring.md:215,317`, package README `:96`; **no** `tddy-tools`/daemon hit |
| Grep | `excused\|Excused` | `packages/` `--include='*.rs'` | `tddy-index-daemon/src/queries.rs:305-307`, `render.rs:182`, `tddy-tools/src/index_console.rs:298,389`, `console.rs:214,235` |
| Grep | `notes`, `.report` | `runner/`, `edit.rs` | `entry_points.rs:203-210` prints both; `rehearsal.rs` drops both |
| Grep | `item_move::outline` on impl | `item_move/outline.rs` | refuses a range inside an `impl` (whole-work discovery `:115`), so not reusable for impl members |
| Grep | `rust-analyzer` test group | `.config/nextest.toml` | `:78-103`: a per-binary filter lists the live binaries |

### Inspected files

#### `packages/tddy-code-restructuring/src/backends/rust/signature_rewrites/call_site.rs`

**Why**: the only shape of "one call, text-only" in the engine.
**Excerpt**:

```rust
pub(in super::super) fn rewrite_call(text: &str, op: &RefactorOp, range: Range) -> Result<Vec<TextEdit>> {
    let call = call_in(text, range)?;                       // :99
    ...
}
fn call_in(text: &str, range: Range) -> Result<Call> {
    ...
    let arity = match syn::parse_str::<syn::Expr>(call_text) {
        Ok(syn::Expr::Call(call)) => call.args.len(),
        Ok(syn::Expr::MethodCall(call)) => call.args.len(),
        _ => return Err(not_a_call()),                      // "is not a call expression — the range must cover exactly one call"
    };
    let code = masked_to_code(text);
    let open = opening_of_the_last_group(code.as_bytes(), from, to).ok_or_else(not_a_call)?;
    ...
    if arguments.len() != arity { return Err(server_defect(...)); }
```

`Call { open, arguments }` is private; `call_in` is private; `rewrite_call` is `pub(in super::super)`.

#### `packages/tddy-code-restructuring/src/backends/rust/item_move.rs` (`sites_of`)

**Why**: the references path a bulk form reuses.
**Excerpt**:

```rust
pub(super) fn sites_of(&mut self, uri: &str, workspace: &Workspace<'_>, file: &str, source_text: &str,
                       named: &[(&str, &Value)]) -> Result<Vec<Site>> {          // :141
    for (name, position) in named {
        let references = self.references_at(uri, position)?;                     // includeDeclaration: false
        for reference in references.as_array().into_iter().flatten() {
            ...
            let offset = lsp_edits::offset_of(&texts[&path], at);
            sites.push(Site { path, offset, name: (*name).to_string() });        // the NAME's offset, one per reference
```

#### `packages/tddy-code-restructuring/src/item_anchor.rs` (`range_within`)

**Why**: how a bulk anchor yields the position `references_at` needs.
**Excerpt**:

```rust
match (start, end) {
    (None, None) => return Ok(Range { start: resolved.name, end: resolved.name }),   // :166-173, zero-width at the name
```

#### `packages/tddy-code-restructuring/src/plan.rs` and `plan/codec.rs`

**Why**: the vertical slice and its constraints.
**Excerpt**: `RefactorOp` at `:381` (`#[serde(deny_unknown_fields)]`, 14 fields, no `Default`); `RefactorKind::edits_a_call_site` (`:289`) is true for the four
argument operations only; `signature_fields::refuse_a_call_site_anchored_off_a_call` requires `Anchor::Item { start: Some, end: Some, .. }` for those four. `plan.rs` is 520
production lines (`mod tests` at `:522`), `codec.rs` 514 (`mod signature_fields;` at `:494`).

#### `packages/tddy-code-restructuring/src/verify.rs`, `verify/statements.rs`

**Why**: whether a re-pointed receiver is accounted for.
**Excerpt**:

```text
4. Re-point pairing: a lost and a gained statement, 1:1, equal once lowercase module qualifiers are deleted (`f(` becoming `m::f(`).
5. Token multiset, last resort: ... identifiers, numbers, string and char literals ... dropping whitespace, `{` `}` `,` `;` and lowercase module qualifiers. ...
   It cannot excuse a renamed callee, a changed or dropped argument, a lost statement or a lost comment.
```

`strip_qualifiers` deletes `segment::` only (`statements.rs:210-260`); nothing deletes `.segment`.
`Excused { repointed, visibility, cfg_test_gates }` (`verify.rs:50-60`) is read by `queries.rs:305-307`.

#### `packages/tddy-code-restructuring/src/runner/rehearsal.rs`

**Why**: what `check --deep` shows.
**Excerpt**: `Ok(resolved) => { self.ledger.record(&resolved.edit); self.overlay.record(root, &resolved.edit)?; Ok(Rehearsed { survey, refusal: None }) }` —
`resolved.notes` and `resolved.report` are not carried.

#### `.config/nextest.toml`, `.config/rust-e2e.filterset`

**Why**: where a live binary must be registered.
**Excerpt**: header `:18-25` ("A test that ... drives a live rust-analyzer ... belongs in that file"); `[test-groups] rust-analyzer = { max-threads = 1 }` with a per-binary
`filter` at `:88-103`. The filterset lists `package(tddy-code-restructuring) and (binary(...) or ...)`; `signature_rewrites_acceptance`, `move_item_*`, `reparent_module_*` are in neither.

#### The two todos

The #532 todo (`no-operation-re-points-a-calls-receiver-or-writes-a-delegator`) describes 14 callee/receiver sites in moved bodies, 14 callers outside, 7 delegators and 2 dead
wrappers, done by hand; it asks for `repoint_call` (item anchor + relative range + `callee` text parsed as one chain; "a bulk form over every reference to a method ... would turn 28
edits into one") and for `leave_delegator` as a variant of `retarget_impl`. Its examples: `self.common_room_slot(x)` -> `self.peer_routing.common_room_slot(x)`; `x.m(..)` -> `x.agent_roster().m(..)`;
`self.session_dir_for(id)` -> `session_dir_lookup::session_dir_for(&self.tddy_data_dir, id)` (an argument list change as well). The master todo is about `self.<field>` -> `state.<field>`
(a field read, with an inserted `let state = …` and a dropped `&`): a different edit from a call.

### Findings

- No code path reads or writes the span in front of a call's `(`; the argument machinery is reusable through a visibility widening.
- A bulk form is `sites_of` plus a receiver reader; the lowered item anchor gives the references position for free.
- `RefactorOp` literals make every added field a 20-site edit shared with `retarget-impl`: one field is cheaper than two.
- `verify` cannot pair an inserted receiver hop; a new pass inside `compare` is the only place to teach it, and the pairs must be counted under `repointed` (wire shape).
- `check --deep` forwards no notes today; that plumbing belongs to `repoint-facade`.
- A new live binary is registered in two files, not one.

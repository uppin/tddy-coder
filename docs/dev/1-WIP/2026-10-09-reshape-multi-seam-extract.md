# Changeset: multi-seam plans resolve every operation against the files the plan has already written

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Bug fix / feature (check/apply parity for multi-seam `extract_module` plans; no new operation, field, flag or wire message)
**Stack**: `#reshape` 2/19, branch `feature/reshape/multi-seam-extract`, green wave 1. PR title:
`fix(code-restructuring): multi-seam plans see the files earlier seams wrote (#reshape 2/19)`.
Base in the linear stack: `feature/reshape/widen-same-crate` (K=1). **Real edges**: none into this node; out of it `multi-seam-extract → oversized-files` (K=15) and `multi-seam-extract → rust-backend-split` (K=17).
K=1 is the base only because `gh stack` needs a line; nothing of it is consumed.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-multi-seam-extract-initial-discovery.md) (Exploration 1: whole backlog; Exploration 2: this node).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no claim on the files this node touches: **no 🚧 claimed issue is in the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md](../todo/2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md) | ✅ **RESOLVED HERE** | All three rows and the parity claim: the server is shown the plan's projected tree (rows 2, 3, and row 1's sibling-file half; row 1's in-body half was already fixed by `inline_paths`), and `check --deep` prints widenings. Its third "possible fix", type-checking the projected tree, moves to the new todo `2026-10-09-restructure-check-deep-does-not-compile-the-projected-tree.md`. Deleted at wrap |
| [2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md) | ⚠ **partial**: J ✅ here | J (sibling seam's private helper unresolved) has the same root cause as 09-18 row 3 and is closed by acceptance tests 3–5. I and K are `#reshape` 4's. **Narrowed at wrap** to what remains: I, K (until 4 wraps), M, W, G (unverified, unclaimed), L (re-measure after this node), and X's second half, which moves to the new todo `2026-10-09-restructure-extract-module-widens-to-pub-crate-where-pub-super-suffices.md`. H is removed as fixed (`inline_paths`) |
| [oversized-file-backends-rust.md](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md) | ⚠ **DURING** | `rust.rs` gains two fields, one `mod` line and a 3-line change to `resolve`. All logic in new `backends/rust/projection.rs`. History row at wrap |
| [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | Same |
| [2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md), [2026-09-24-restructure-apply-leaves-the-lint-gate-red.md](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md) | — Unrelated | `#reshape` 4's; this node changes nothing in `extract_method` or the lint gate |
| [2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md](../todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md) | — Unrelated | `#reshape` 10's wait deadline. This node adds no wait |
| `packages/tddy-code-restructuring/docs/code-issues/*` others | — | Not in the path |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md); new `src/backends/rust/projection.rs`; `src/backends/rust.rs` (`mod projection;`, two `RustBackend` fields and their initialisers, the `resolve` trait method); `src/backends/rust/documents.rs` (`did_open` flushes the staged projection, `close_opened` drops it); `src/runner/rehearsal.rs` (`Rehearsed.widenings`, new `rehearsed_lines`); `src/runner/entry_points/check_entry_points.rs` (the deep-check loop prints through `rehearsed_lines`). Tests: new `tests/multi_seam_extract_acceptance.rs`, new `tests/multi_seam/mod.rs`. Registration: `.config/rust-e2e.filterset`, `.config/nextest.toml`.
  Docs at wrap: [assist-output-repairs.md](../../../packages/tddy-code-restructuring/docs/assist-output-repairs.md) (a seam after a seam), [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md) (a row: projected documents), [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md).
- **`tddy-index-daemon`, `tddy-tools`**: no source change. The daemon's apply loop (`tddy-index-daemon/src/apply.rs`) builds one backend per run and gets the behaviour through it.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-multi-seam-extract.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-multi-seam-extract.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `### Moved code that changes meaning one module deeper`, `## What a run executes`

## Summary

Before any operation of a run asks rust-analyzer a question, the Rust backend opens every file earlier operations of that run created or changed, with the text the run holds for it — the overlay's in `check --deep` and `apply --dry-run`, the disk's in `apply`. The existing passes then see sibling seams: a reference from an earlier seam's file keeps a moved item widened (and reported), is imported where a later seam needs it, or is refused as stranded when no facade is asked for. `check --deep` prints each widening in the same form `apply` does.

## Background

`#carve` 3 cut eight seams from `crate_move.rs` in one plan: `check --deep` clean, `apply` 8 of 8, 13 compile errors. `#carve` 13's nine-seam plan left `use super::strip_resize;` dangling (gap J). The engine already treats a reference in another file as reached from outside (`reach_of`), but only the anchor's document is opened. In a rehearsal the earlier seams' files exist only in the `Overlay`, which the server never sees. In an apply they are on disk, and the engine relies on rust-analyzer's own watcher, which `tddy-index-daemon/src/tree_changes.rs` records as blind on this workspace within a request. `documents.rs`'s header records the same hazard from the other side: a rehearsed parent left open named modules "whose files existed only in that check's overlay".

## Responsibility

- A per-run record of the files the backend's resolutions created, changed or renamed into (`PlanProjection`), folded after each successful `resolve`.
- Showing those files to the server, at their projected text, before the operation's first document is opened, for **every** operation the Rust backend resolves (F2); closing them with every other document when the operation ends.
- Carrying `Resolution::report` into `Rehearsed` and printing it in `check --deep` as `apply` prints it.
- One new live binary, registered in `.config/rust-e2e.filterset` and the `rust-analyzer` group of `.config/nextest.toml`.

## Rules (the contract)

1. **What is recorded.** After `RustBackend::resolve` returns `Ok`, for each `FileEdit` of its edit, in order: `Create { path }` and `Change { path, .. }` add `path`; `Rename { from, to }` removes `from` and adds `to`. A path is listed once, at the position it was first added. A refused operation records nothing. The record lives as long as the backend, which is one run (one `BackendRegistry` per run in the runner, the rehearsal and the daemon).
2. **What is shown.** At the start of `resolve`, every recorded path is read through `workspace.read(path)` and staged. The first `did_open` the operation makes sends `textDocument/didOpen` for every staged document except the one being opened, then the requested one. A later `did_open` of a uri already open from the projection sends the requested text as a full `textDocument/didChange` instead of a second `didOpen`. A staged path that `workspace.read` cannot read is an error naming the path (no fallback: a recorded file the run cannot read means the record and the tree disagree).
3. **What is closed.** Projected documents are pushed onto `opened` like any other, so `closing_what_it_opens` closes them; the staging is cleared there too, so nothing staged for one entry point reaches the next (`anchor_for`, `module_references`, `item_resolver`).
4. **Nothing else changes in the passes.** `reach_of`, `restore_imports`, `restore_visibility`, `refuse_stranded`, `inline_paths` are unchanged; they get a server that answers for the projected tree.
5. **Deep-check output.** `Rehearsed` gains `widenings: Vec<VisibilityChange>` (from `Resolution::report`). `rehearsed_lines(index, &Rehearsed)` returns, in order, the survey lines (as today), one `console::visibility(&console::widening(change))` line per widening, then one `console::note` line per note. `check_plan`'s loop prints those lines; findings are unchanged.
6. **Single-seam plans.** The first operation of a run has an empty record, so it sends exactly what it sends today.

## Boundaries

- **No rewrite of references in files other than the anchor's.** A stranded sibling reference is refused (F3), not edited.
- **No compile of the projected tree in `check --deep`** — the new todo.
- **No `workspace/didChangeWatchedFiles`**, no new client capability, no re-indexing or new wait between operations.
- **No change to `Workspace`, `Overlay`, the runner's or the daemon's apply loops, or the plan format** (F1).
- **No change to widening width** (`pub(crate)`); X's second half is a new todo.
- **No growth of any function over 60 lines** (the `#reshape` 16/19 lists): `resolve_opening` and `assisted_edit` are not touched; `check_plan`'s loop shrinks.
- Registration only of the binary this node adds; the unregistered existing seam suites are `#reshape` 1's.

## Dependencies

This node has no parent: it consumes no behaviour from any other `#reshape` node, and `widen-same-crate` is its base only because the stack is a line.

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR after planning); **owned surface, new today**, all crate-private:

- `backends::rust::projection::PlanProjection` — `#[derive(Debug, Default, Clone, PartialEq, Eq)] pub(super) struct PlanProjection { written: Vec<String> }` with
  `pub(super) fn record(&mut self, edit: &WorkspaceEdit)` and `pub(super) fn paths(&self) -> &[String]`.
- `impl RustBackend` in `projection.rs`: `pub(super) fn stage_projection(&mut self, workspace: &Workspace<'_>) -> Result<()>` and `pub(super) fn open_staged_projection(&mut self, except: &str) -> Result<()>`.
- `RustBackend` fields `projection: PlanProjection` and `staged_projection: Vec<(String, String)>` (uri, text).
- `runner::rehearsal::Rehearsed::widenings: Vec<VisibilityChange>`; `runner::rehearsal::rehearsed_lines(index: usize, rehearsed: &Rehearsed) -> Vec<String>`.
- Test support: `tests/multi_seam/mod.rs` fixtures (`a_crate_whose_pty_handle_calls_strip_resize`, `a_crate_whose_mover_calls_relative_from`, `a_crate_cut_three_ways_like_carve_3`), op builders, and `resolving_in_order(fixture, ops) -> Result<ResolvedInOrder, String>` (one backend, one `PositionLedger`, one `Overlay`, as `Rehearsal::rehearse` does; `ResolvedInOrder::text(path)` reads the overlay).
- Failing tests: acceptance tests 1, 3, 4, 6, 8 and unit tests 10–15 below; 2, 5, 7, 9 are pins (see each).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes; nothing it consumes is in the stack.
**Concurrent with:** every other wave-1 node (`widen-same-crate`, `tidy-facades`, `extract-method-clean`, `move-children`, `methods-leave-type`, `move-widen`, `move-grouped-use`, `new-crate`, `apply-robust`, `move-item-paths`, `anchors-outline`). Textual overlap only: `backends/rust.rs` (fields, `mod` list) and `runner/rehearsal.rs` / `check_entry_points.rs` with `apply-robust` and `anchors-outline`.
**Blocks:** `oversized-files` (K=15), `rust-backend-split` (K=17).
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`, `4→19`, `17→19`.

## Successor PRs

- `feature/reshape/oversized-files` — splits `crate_move/test_binary.rs` and `item_move/assemble.rs` with multi-seam `extract_module` plans; relies on `check --deep` vouching for them.
- `feature/reshape/rust-backend-split` — splits `backends/rust.rs` with many seams in one plan.

## Scope

- [ ] **Projection record**: `PlanProjection` and its fold (rule 1)
- [ ] **Showing it**: staging in `resolve`, flush in `did_open`, `didChange` for a re-open, clear on close (rules 2–3)
- [ ] **Deep-check widenings**: `Rehearsed.widenings`, `rehearsed_lines`, `check_plan` prints through it (rule 5)
- [ ] **Live acceptance binary** and its registration in both config files
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, clippy `-D warnings`, `cargo fmt`; no function over 60 lines grown

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `resolve` (`rust.rs:1207-1209`) runs `resolve_opening` inside `closing_what_it_opens`; `resolve_opening` opens only the anchor (`rust.rs:1364`, `:1376`). Cross-crate and same-crate moves open their own module documents.
- `reach_of` (`rust.rs:1833-1872`) already counts other-file references as `stranded_in` + `from_outside` (`:1849`); `restore_visibility` keeps a widening on `reached_from_outside` (`visibility.rs:82`); `restore_imports` skips names already unresolved before the cut (`imports.rs:57`).
- `Rehearsal::rehearse` keeps earlier edits in an `Overlay` (`rehearsal.rs:62-66`) that no server sees; `apply --dry-run` (`store_run.rs:322-333`) and the daemon (`tddy-index-daemon/src/apply.rs:190-194`) the same. A real apply depends on rust-analyzer's watcher; the client advertises no watched-files capability (`rust.rs:196-236`).
- `Rehearsed` has `survey`, `refusal`, `notes` (`rehearsal.rs:19-27`); `check_plan` prints survey lines and notes (`check_entry_points.rs:296-303`), never widenings, which `apply` prints via `report_visibility` (`entry_points.rs:218-227`).
- Per-run plan state precedent: `RustBackend::claimed` (`rust.rs:473-481`).

### State B (Target)

Every server question of operation `k+1` is answered for the tree ops `1..k` produced, in all three modes. A multi-seam plan's `check --deep`, `apply --dry-run` and `apply` reach the same refusals and print the same widenings.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **New** `src/backends/rust/projection.rs` (~90 production lines): `PlanProjection`, `stage_projection`, `open_staged_projection`, unit tests.
- **`rust.rs`**: `mod projection;`; fields `projection`, `staged_projection` in the struct and its two constructors (`:533`, `:625` neighbourhoods); `resolve` becomes stage → `closing_what_it_opens(resolve_opening)` → `projection.record(&resolution.edit)` on `Ok`.
- **`documents.rs`**: `did_open` calls `open_staged_projection(uri)` first while staging is non-empty, and sends `didChange` for a uri already open; `close_opened` clears `staged_projection`.
- **`runner/rehearsal.rs`**: `Rehearsed.widenings` filled from `resolved.report`; new `rehearsed_lines`.
- **`runner/entry_points/check_entry_points.rs`**: the survey/notes printing in `check_plan` is replaced by one loop over `rehearsed_lines`.
- **`tests/multi_seam/mod.rs`**, **`tests/multi_seam_extract_acceptance.rs`**: new. **`.config/rust-e2e.filterset`**: `or binary(multi_seam_extract_acceptance)`; **`.config/nextest.toml`**: the same binary in the `rust-analyzer` group's filter.

## Implementation milestones

- [ ] **M1** spike: one live test that rust-analyzer resolves `mod x;` in an open parent to a file that exists only as another open document (acceptance test 6 is that test); if it does not, stop and ask the developer (the fallback would be materialising the overlay, a different design)
- [ ] **M2** `PlanProjection` and its fold; unit tests 10–13
- [ ] **M3** staging, flush, re-open as `didChange`, clear on close; unit test 14; acceptance tests 1, 3, 6, 9
- [ ] **M4** deep-check widenings; acceptance tests 4, 8; unit test 15
- [ ] **M5** apply pins 2, 5, 7; registration in both config files
- [ ] **M6** docs staged, scoped gate, function-size check

## Testing plan

### Testing Strategy

**Live rust-analyzer, rehearsal first.** The defect is in what the server is shown, so the oracle has to be a real server. The deterministic reds go through a rehearsal (`check --deep`, `apply --dry-run`, or `resolving_in_order`): there the earlier seams' files are never on disk, so master is blind on every machine. On a small fixture rust-analyzer's own watcher sees files a real `apply` wrote, so the apply tests (2, 5, 7) can already pass on master: they are regression pins that the new mechanism must not break, not red evidence (F4).
**Unit level** for the record's fold and the deep-check line rendering, no server.

#### Option 1 (chosen): new live binary with its own fixture module
Fixtures in `tests/multi_seam/mod.rs`, following the harness's `OUTER_MODULE` pattern (`crates/origin/src/outer.rs`, item-anchored `extract_module --to_file`), built on `harness::{a_workspace_of, a_manifest_for, applying_a_plan_of, checking_the_plan_with, resolving_after_a_check_of, assert_compiles, assert_compiles_with_its_tests}`.
- `a_crate_whose_pty_handle_calls_strip_resize`: `outer.rs` holds `pub(crate) struct PtyHandle { pub(crate) data: Vec<u8> }`, `impl PtyHandle { pub(crate) fn send_input(&self) -> usize { strip_resize(&self.data) } }`, private `fn strip_resize(data: &[u8]) -> usize`, and `pub(crate) fn total()` calling `send_input`. Seam 1: `PtyHandle` + `impl PtyHandle` → `pty_handle`, `reexport: glob`. Seam 2: `strip_resize` → `resize`, `reexport` per test.
- `a_crate_whose_mover_calls_relative_from`: private `fn relative_from(..)`, `pub(crate) fn moving(..)` calling it bare, `total`. Seam 1: `relative_from` → `paths` (glob). Seam 2: `moving` → `moving_files` (glob).
- `a_crate_cut_three_ways_like_carve_3`: the first fixture plus `pub(crate) fn refusals(handle: &PtyHandle) -> bool` calling `strip_resize` too, and a `#[cfg(test)] mod tests` using `total`. Seams: `pty_handle`, `refusals`, `resize`, all glob.
**Location**: `packages/tddy-code-restructuring/tests/multi_seam_extract_acceptance.rs`.

#### Option 2 (rejected): `fake_lsp` recording `didOpen`
Would prove the documents are sent, not that the passes then decide correctly; the fake answers no references.

#### Option 3 (rejected): blind the watcher in the harness (`files.watcher: client` + a dynamic-registration capability)
Makes the apply tests red on master, but adds harness-only server configuration and depends on `tddy-lsp` answering `client/registerCapability` (F4).

### Coverage Requirements

- [ ] Visibility kept and reported across a sibling file (gap J), in dry run, deep check and apply
- [ ] Stranded sibling reference refused with `reexport: none`, deep check and apply
- [ ] Import of a helper an earlier seam moved, without writing and with writing
- [ ] Three-seam parity of widenings between `check --deep` and `apply`; compiles with tests
- [ ] A shared server is left with no projected document open
- [ ] Record fold: create, change, rename, order, duplicates; staging skips the opened uri

## Acceptance tests

Names read as behaviour specifications. **Red on `master`**: 1, 3, 4, 6, 8 (and 10–15, which do not compile without the owned surface). **Pins**: 2, 5, 7, 9 — they may pass on master and must stay green.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/multi_seam_extract_acceptance.rs` (new live binary; registered in `.config/rust-e2e.filterset` and the `rust-analyzer` group)

1. `a_deep_check_refuses_a_seam_that_strands_a_reference_from_a_file_an_earlier_seam_wrote` — `a_crate_whose_pty_handle_calls_strip_resize`, seam 2 `reexport: none`; the one finding names `strip_resize` and `crates/origin/src/outer/pty_handle.rs`. *Red on master*: `no findings` — `pty_handle.rs` is overlay-only, so `reach_of` sees no other-file reference.
2. `an_apply_refuses_the_same_seam_after_applying_the_first` — same plan through `applying_a_plan_of`: the run fails at op 1 (the second, zero-based as the runner counts) with the same refusal text, `pty_handle.rs` exists, `resize.rs` does not. *Pin* (the small fixture's watcher may already show the server `pty_handle.rs`).
3. `a_dry_run_reports_the_widening_a_file_an_earlier_seam_wrote_forces` — seam 2 `reexport: glob`, `apply --dry-run` through the runner capturing `options.account`; contains the `visibility:` line for `strip_resize`, `private` → `pub(crate)`. *Red on master*: the item is narrowed back to private and no line is printed (gap J).
4. `a_deep_check_reports_the_same_widenings_a_dry_run_reports` — `checking_the_plan_with(deep)` capturing `options.account`; its `visibility:` lines equal test 3's. *Red on master*: `check --deep` prints no widenings at all, and the rehearsal narrows.
5. `a_plan_whose_earlier_seam_reaches_what_a_later_seam_moves_applies_and_compiles` — same glob plan applied: `applied == 2`, `assert_compiles`, `resize.rs` declares `pub(crate) fn strip_resize`. *Pin*.
6. `a_later_seam_imports_a_helper_an_earlier_seam_moved_when_the_plan_is_resolved_without_writing` — `a_crate_whose_mover_calls_relative_from` through `resolving_in_order`; `crates/origin/src/outer/moving_files.rs`'s projected text has a `use` binding `relative_from` (`use super::relative_from;` or `use super::paths::relative_from;`, whichever the server offers first — asserted as "one `use` line whose last segment is `relative_from`"). *Red on master*: no import — the name was already unresolved in the parent's overlay text, so `restore_imports` treats it as not lost. Doubles as M1's spike.
7. `the_same_plan_applied_imports_the_helper_and_compiles` — applied: `applied == 2`, `assert_compiles`. *Pin*.
8. `a_three_seam_plan_of_the_carve_shape_widens_the_same_items_in_its_deep_check_and_its_apply_and_compiles_with_its_tests` — `a_crate_cut_three_ways_like_carve_3`: deep check has no findings; its `visibility:` lines equal the apply's; the apply applies 3 of 3; `assert_compiles_with_its_tests`. *Red on master*: the deep check prints no widenings.
9. `a_check_leaves_no_projected_document_open_on_the_server_it_shares` — `resolving_after_a_check_of(the two-seam plan with seam 2 glob, then seam 2 alone with reexport: none on the untouched tree)` returns `Ok`: had `pty_handle.rs`'s rehearsed text stayed open, the lone seam would be refused as stranded. *Pin* (nothing is projected on master); guards rule 3.

### `tddy-code-restructuring` — unit tests in `packages/tddy-code-restructuring/src/backends/rust/projection.rs` (`#[cfg(test)] mod tests`, no server)

10. `records_every_file_an_edit_creates_or_changes`
11. `follows_a_rename_to_its_new_path_and_forgets_the_old_one`
12. `lists_each_path_once_in_the_order_the_plan_first_wrote_it`
13. `an_empty_record_stages_nothing_so_a_first_operation_sends_what_it_sent_before`
14. `staging_reads_each_path_through_the_workspace_and_an_unreadable_path_is_an_error_naming_it` (a `Workspace` over a temp dir and an `Overlay`; no server needed for staging)

### `tddy-code-restructuring` — unit test in `packages/tddy-code-restructuring/src/runner/rehearsal.rs` (`mod tests`)

15. `prints_each_widening_between_the_survey_and_the_notes_in_the_form_apply_prints` — `rehearsed_lines` over a `Rehearsed` with one survey, two widenings and one note; exact lines.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (brief, 2026-10-09): "All 12 wave-1 PRDs approved. Every node's own F-decisions: take the agent's recommendation"; "Other nodes register only the suites they add"; "nodes must not grow functions on nodes 16/19's list … put new logic in new functions".

**Taken** (recommendations approved):
- **F1 — where the record lives.** **On `RustBackend`**, beside `claimed`, folded from each `Resolution`, with texts read through `workspace.read` — no change to `Workspace` (~25 struct literals incl. `tddy-index-daemon`) or to either apply loop. Rejected: tracking written paths in `Overlay`/`Workspace`.
- **F2 — which operations.** **Every operation the Rust backend resolves**, through the `did_open` choke point — one rule, and it may bear on 09-24 L. Rejected: `extract_module` only.
- **F3 — a stranded sibling reference.** **Refuse** through the existing `refuse_stranded`. Rejected: engine-authored rewrites in files other than the anchor's.
- **F4 — red evidence.** **Rehearsal tests are the reds; apply tests are pins.** Rejected: harness-only watcher configuration.

**Taken by this plan:** a re-open of a projected uri is a `didChange`, not a second `didOpen`; an unreadable recorded path is an error, not a skip; the first operation of a run is byte-for-byte unchanged.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

(empty; populated by `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`)

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-multi-seam-extract-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-multi-seam-extract.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — deletes the 09-18 todo and this node's discovery file, narrows the 09-24 todo (removes H and J), adds history rows to the two DURING items
- [ ] USER REVIEW — work complete, decide next steps

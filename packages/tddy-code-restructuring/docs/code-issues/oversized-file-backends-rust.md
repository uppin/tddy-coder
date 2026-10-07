# oversized-file: backends/rust.rs — the whole rust-analyzer backend in one module

**Location:** `packages/tddy-code-restructuring/src/backends/rust.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate on #498
**Metrics:** **3,008 production lines** (2026-10-07, `#sharpen` 8/8 repoint-facade; 2,997 at 2026-10-07, `#sharpen` 7/8 repoint-call; 2,986 at 2026-10-07, `#sharpen` 6/8 retarget-impl; 2,975 at 2026-10-06, `#sharpen` 4/8 apply-heartbeat; 2,864 at 2026-10-06, `#sharpen` 3/8 spawn-record; 2,853 at 2026-10-05, same-crate moves; 2,831 at 2026-10-04, #567; 2,705 at #569; 2,666 after #539; 4,475 before; 4,475 after #542, 4,433 after #537; 4,360 after #526, 4,342 after #524, 4,788 before #527) · budget 500
**Restructure:** required
**Status:** Open — partially fixed (the impl-member seams remain). #539 (`#live-plan` 7/15) moved the free-item runs out (4,475 to 2,666); #527 had already taken 472 net

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 4,788 | first detection; 4,772 before #498 |
| 2026-09-23 | 4,294 | #527 (engine fixes) changed the import pass, the `impl`-seam refusal and the readiness waits, and moved each into a sibling under `backends/rust/` rather than growing this file: `imports.rs`, `impl_seam.rs`, `chatter.rs` (`ServerChatter`, re-exported at its old path), `readiness.rs`. Still 8.6× the budget |
| 2026-09-24 | 4,307 | #527's three explicit-failure guards. The logic went to siblings (`early_return.rs`, `chatter.rs`, `readiness.rs`, and `runner/compile_gate.rs` outside this file); the +13 here is wiring only: the `mod`/`use` lines and the two `refuse_early_returns` call sites in `check` and `resolve` |
| 2026-09-24 | 4,316 | #527 wrap re-measure, after gaps A–C: the repairs went to `imports.rs`, `impl_seam.rs` and a new `nested_modules.rs`; the +9 here is the `nested_modules` wiring and one reworded refusal. 8.6× the budget — `rust.rs` shrank only because new logic went elsewhere, and none of its own seams were cut |
| 2026-09-25 | 4,342 | #524 (`#carve` 14/15), `3714a654`: every entry point now closes the documents it opened. `did_open` and the closing went to a new sibling, `documents.rs` (59 lines); the +29 here (4,313 → 4,342 at `3714a654^` and after, both by the inline-test-block rule) is the three `closing_what_it_opens` wrappers around the bodies moved verbatim into `resolve_opening`, `anchor_opening` and `outside_references_opening`, and the `opened` field. Unchanged in kind: none of its own seams were cut |
| 2026-09-26 | 4,360 | #526 (`#carve` 15/21): +18 by this wrap's re-implementation of the inline-test-block rule, which reads `2688227f` as 4,340 and `22787218` as 4,358. +5 in `5446cec6` (the inactive-code answer threaded through the caller survey) and +13 in `cb367ac6` (`carries_placeholder_type` reading `'_` as a lifetime); `cc19d3a4` net 0 (the `extract_variable` naming went to a new sibling, `introduced.rs`). Regressed slightly; none of its own seams were cut |
| 2026-10-02 | 4,433 | #537 (`#live-plan` 1/7): +74 by the same rule (merge base 4,359 → 4,433). Item-anchor resolution through the LSP outline (including the outline-readiness check `outline_is_the_servers_answer`); the split is deferred because later `#live-plan` nodes touch this file. None of its own seams were cut |
| 2026-10-02 | 4,475 | #542 (`#live-plan` 5/7): +42 by the same rule (merge base 4,433 → 4,475). `assist` takes a probe position, `assisted_edit` widens/refuses a borrowed selection, probes within the bound and carries function-local `use` items, and `check` reports the carry; the logic itself went to `selection.rs` (new), `imports.rs`, `early_return.rs` and `readiness.rs`. Split deferred: #543 (`check-parity`, the dependent) touches this file. None of its own seams were cut |
| 2026-10-03 | 2,666 | #539 (`#live-plan` 7/15): 4,475 to 2,666 (−1,809) by engine moves only. Nine modules took the free-item runs: `line_diff`, `placeholder_checks`, `lsp_edits`, `import_text`, `module_text`, `visibility`, `seam_survey`, `facade`, `server_process`. What is left is the methods of the three `impl RustBackend` blocks and the trait impls, which only the engine's impl-member seam can move |
| 2026-10-03 | 2,705 | #569 (`#live-plan` 9/15): +39. The two signature assists' own logic (carets, the used-parameter refusal, the struct rename) went to a new sibling, `backends/rust/signature.rs` (403 lines); what is left here is wiring: two `SUPPORTED` entries, two `assist_for` rows and the dispatch in `multi_file_assist`. The split of this file stays deferred — every open `#live-plan` node edits it — with the developer's consent. |
| 2026-10-04 | 2,831 | #567 (`#live-plan` 15/15): +125 over the pre-PR 2,706. The eight signature and call-site operations' logic went to `backends/rust/signature_rewrites.rs` (170 production lines) and its children `declaration.rs` and `call_site.rs`, and `return_type_assist` to `return_type.rs`. What stays is the impl members `rewrite_signature`, `wrap_or_unwrap_return_type` and `offered_assist` and two resolve branches: `restructure check --deep` refused moving them (`rust-analyzer` cannot put a `mod` inside an `impl` body). Deferred with the developer's consent; the impl-member seam is `docs/dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md` § 1. |
| 2026-10-05 | 2,853 | `move_item` and `reparent_module` (same-crate moves): +22 over 2,831, counted to the first inline `#[cfg(test)] mod` (2,830 to 2,852 by the same awk at the merge base and at HEAD). Wiring only: `mod item_move;` and `mod module_reparent;`, two `SUPPORTED` entries (20 to 22), a `check` arm per operation and a `resolve` arm per operation. All logic is in `backends/rust/item_move/` and `backends/rust/module_reparent/`. The impl-member seams remain; none was cut |
| 2026-10-06 | 2,864 | `#sharpen` 3/8 (`spawn-record`): +12 over the 2,852 at `235e9e4e`, counted the same way. Wiring only: `use crate::spawn_record::{purpose, SpawnRecorder}`, the `spawns` field with its doc, the `with_spawn_recorder` builder and its two constructor lines, and `start`'s `self.spawns.spawn(purpose::RUST_ANALYZER, …)` call (the `Command::new` there became `self.spawns.command`). The recorder itself, its JSONL sink and its redaction live in `spawn_record.rs` and its children. None of the impl-member seams was cut |
| 2026-10-06 | 2,975 | `#sharpen` 4/8 (`apply-heartbeat`): +111 over the 2,864 at `#sharpen` 3/8, counted the same way. The wait heartbeat: `backends/rust/wait.rs` (new), the `wait_heartbeat` field and `with_wait_heartbeat` builder, `keep_waiting`'s beat, and the five waits (`ensure_indexed`, `await_answer`, the assist wait, `settled_outline`, `locate_symbol`) naming their stage; `incomplete_assist_index` gained the stage. `Waiting` and `heartbeat_line` live in `wait.rs`; what is here is the wiring and the call sites. The split stays deferred: the sibling `#sharpen` nodes still touch this file, and the impl-member seams remain |
| 2026-10-07 | 2,986 | `#sharpen` 6/8 (`retarget_impl`): wiring only, counted to the first inline `#[cfg(test)] mod` (line 2987). `mod retarget_impl;`, `SUPPORTED` 22 → 23, one `check` arm and one `resolve` arm — about 11 lines. All logic is in `backends/rust/retarget_impl/` and its children (`outline.rs`, `rewrite.rs`, `fields.rs`, `imports.rs`, `preflight.rs`), so the growth this node adds here is a `mod` line and four arms. The impl-member seams remain; none was cut |
| 2026-10-07 | 2,997 | `#sharpen` 7/8 (`repoint_call`): wiring only, counted the same way. `RefactorKind::RepointCall` in `SUPPORTED`, one `check` arm and one `resolve` arm — about 11 lines. All logic is in `backends/rust/repoint_call.rs` and its children (`single.rs`, `sites.rs`, `receivers.rs`). The split stays deferred: the sibling `#sharpen` nodes still touch this file, and the impl-member seams remain. `/pr-wrap`'s file-length gate flagged it, and the stack-overlap stop (parent `retarget-impl` and dependent `repoint-facade` both edit this file) defers the split to a follow-up after the stack lands |
| 2026-10-07 | 3,008 | `#sharpen` 8/8 (`repoint_facade`): wiring only, counted to the first inline `#[cfg(test)] mod` (line 3009). `mod repoint_facade;`, `SUPPORTED` 24 → 25, one `check` arm and one `resolve` arm — about 11 lines. All logic is in `backends/rust/repoint_facade/` and its children (`scope.rs`, `rewrite.rs`, `group.rs`, `refusals.rs`). This is the last `#sharpen` node to touch this file; the split stays deferred — the stack-overlap stop defers it to a follow-up after the stack lands, and the impl-member seams remain. `/pr-wrap`'s file-length gate flagged it (its awk reads to the first `#[cfg(test)]` and so reports 510) |

## What the gate found

Already 9.5× the budget before #498. Its contribution is **16 lines**: `MoveTestBinaryToCrate` added
to the `SUPPORTED` kinds and one dispatch branch, placed before `self.start()` because the operation
needs no language server.

## Why it was not split in #498

The `/pr-wrap` stack-overlap stop. **#491 (`#carve` 5/10 `core-foundations`) also touches this file**,
and it is stacked directly on #498 — splitting the file here would rewrite paths under a PR already
in flight and turn its diff into a conflict.

## What would close it

The free-item seams are cut. What remains is inside `impl RustBackend` and its trait impls
(`Drop`, `LanguageBackend`, `ModuleReferences`): five member runs of roughly 120 to 330 lines each —
the session plumbing, the transport (`start` … `assist`), the extraction methods (`assisted_edit` …
`reach_of`), the assist-driven operations (`prune_assist_imports` … `rename_placeholder`), and the whole
trait impls. They need the engine's impl-member seam, a probe first for each, and a decision about the
`pub(crate)` a moved private method keeps. The sizes, the risks found and the order are in
`docs/dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md` § 1;
when they are moved, re-measure and delete this record with the final number in the change history.

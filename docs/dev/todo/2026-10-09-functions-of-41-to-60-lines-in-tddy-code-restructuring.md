# 2026-10-09 — 71 functions of 41–60 lines in `tddy-code-restructuring`

**Category:** Code quality (deferred function-size band)
**Source:** #reshape 19/19 (`fn-sizes-backend`); band measured by the whole-work discovery of `#reshape`, Exploration 3

`.agents/commands/analyze-clean-code.md` grades a function of 41–60 lines "needs attention" and one of more than 60 "must
refactor". `#reshape` 16 and 19 cut every production function of the crate past 60 lines and added
`packages/tddy-code-restructuring/tests/function_length_budget.rs`, which fails on any function past 60. The band below
was left alone on purpose.

**Count: 71 functions** on `master` at `4a5c42b1b` (2026-10-09), 45 under `src/backends/` and 26 outside it. The count is
from the brace-matching scan (fn line to closing brace, inclusive, production code only), which can differ from the
gate's `syn` measure by a line or two. `facade_lines` (58) leaves the band in `#reshape` 19, cut to close
`complexity-rust-facade-lines.md`. The functions `#reshape` 16 and 19 write land in the band when they are 41–60 lines (for
example `resolve_opening`'s `authored_resolution`, about 46), so re-measure with the gate's walk at that node's wrap and
update this list.

Five sit at exactly 60, one line under the cap: any growth puts them on the gate's list.

| Lines | Function (as of 2026-10-09) | Name |
|---|---|---|
| 60 | `src/backends/rust/readiness.rs:147` | `await_answer` |
| 60 | `src/backends/rust/item_move/rebase.rs:99` | `path_edit` |
| 60 | `src/backends/rust/item_move/assemble.rs:448` | `into_destination` |
| 60 | `src/backends/rust/item_move/assemble.rs:348` | `moved_text` |
| 60 | `src/backends/rust.rs:1933` | `chain_module_to_file` |
| 59 | `src/backends/rust/repoint_facade/group.rs:20` | `split_or_reprefix` |
| 58 | `src/verify.rs:166` | `compare_with` |
| 58 | `src/backends/rust/repoint_call/sites.rs:29` | `repoint_receivers` |
| 58 | `src/backends/rust/facade.rs:151` | `facade_lines` |
| 57 | `src/crate_move.rs:211` | `surveyed` |
| 57 | `src/backends/rust/repoint_facade/rewrite.rs:35` | `path_edits` |
| 56 | `src/plan/codec/signature_fields.rs:43` | `refuse_a_missing_or_unknown_field` |
| 56 | `src/backends/rust/item_move/sites.rs:261` | `short_form` |
| 55 | `src/crate_move/header.rs:67` | `repointed_header` |
| 54 | `src/crate_move/survey.rs:67` | `survey_moved_file` |
| 54 | `src/backends/rust/placeholder_checks.rs:69` | `refuse_residual_placeholder` |
| 53 | `src/runner/compile_gate.rs:91` | `refuse_a_broken_result` |
| 53 | `src/backends/rust/module_reparent/assemble.rs:49` | `assemble` |
| 52 | `src/spawn_record/redact.rs:39` | `redacted` |
| 52 | `src/runner/tidy/gating.rs:387` | `place_statement` |
| 52 | `src/restructure_cli.rs:41` | `run` |
| 52 | `src/backends/rust/item_move/preflight.rs:112` | `obstacles` |
| 52 | `src/backends/rust.rs:1752` | `survey_impl_members` |
| 50 | `src/crate_move/source_scan.rs:171` | `use_tree` |
| 50 | `src/backends/rust/repoint_facade.rs:117` | `use_statement_edits` |
| 50 | `src/backends/rust/lsp_edits.rs:32` | `unresolved_in` |
| 50 | `src/backends/rust/chatter.rs:153` | `progress_at` |
| 49 | `src/runner/rehearsal.rs:30` | `rehearse` |
| 49 | `src/crate_move/moving.rs:156` | `dependency_lines` |
| 48 | `src/verify/repoint.rs:49` | `account` |
| 48 | `src/plan_store/live.rs:229` | `rejudge_anchor` |
| 47 | `src/backends/rust/imports/local_uses.rs:37` | `function_local_uses_reaching` |
| 47 | `src/backends/rust/early_return/body_scan.rs:92` | `punctuation` |
| 46 | `src/verify/retarget.rs:160` | `repeated_headers` |
| 46 | `src/plan_store/live/fold.rs:253` | `follow_changed_item` |
| 46 | `src/plan/codec.rs:48` | `parse` |
| 46 | `src/plan.rs:65` | `validate` |
| 46 | `src/crate_move/header.rs:177` | `rewrite_of` |
| 46 | `src/backends/rust/signature_rewrites/declaration.rs:71` | `add_param` |
| 46 | `src/backends/rust/module_reparent/assemble.rs:125` | `repoint_callers` |
| 45 | `src/runner/tidy/gating.rs:188` | `rewrite_members` |
| 45 | `src/item_anchor.rs:102` | `range_within` |
| 45 | `src/crate_move/moving/facade_writer.rs:60` | `leaving` |
| 45 | `src/backends/rust/retarget_impl/fields.rs:49` | `refuse` |
| 45 | `src/backends/rust/module_reparent/declaration.rs:69` | `find` |
| 45 | `src/backends/rust/item_move/placement.rs:15` | `insertion` |
| 45 | `src/backends/rust/item_move/imports.rs:48` | `header_imports` |
| 45 | `src/backends/rust/item_move/facade.rs:18` | `lines` |
| 45 | `src/backends/rust.rs:2173` | `rename_symbol` |
| 44 | `src/backends/rust/module_reparent/visibility.rs:32` | `landing` |
| 44 | `src/backends/rust/line_diff.rs:102` | `backtrack` |
| 44 | `src/backends/rust/line_diff.rs:56` | `common_runs` |
| 44 | `src/backends/rust/imports.rs:40` | `restore_imports` |
| 43 | `src/runner/group_gate.rs:299` | `roll_back_group` |
| 43 | `src/item_anchor.rs:261` | `covering_run` |
| 43 | `src/backends/rust/module_reparent/survey.rs:96` | `old_parent` |
| 43 | `src/backends/rust/module_reparent.rs:45` | `reparent_module` |
| 43 | `src/backends/rust/item_move/destination.rs:82` | `walk` |
| 43 | `src/backends/rust/item_move/assemble.rs:302` | `repoint_callers` |
| 43 | `src/backends/rust/item_move.rs:144` | `sites_of` |
| 43 | `src/backends/rust.rs:2020` | `multi_file_assist` |
| 42 | `src/backends/rust/retarget_impl/imports.rs:19` | `the_use` |
| 42 | `src/backends/rust/module_reparent/assemble.rs:176` | `rebase_the_moved_files` |
| 42 | `src/backends/rust/lsp_edits.rs:138` | `workspace_edits_for` |
| 42 | `src/backends/rust/facade.rs:76` | `refuse_mangled_rewrite` |
| 41 | `src/plan_store/live/fold.rs:209` | `fold_item` |
| 41 | `src/plan/codec/headerless.rs:57` | `header_for_anchored_files` |
| 41 | `src/backends/rust/repoint_call/sites.rs:118` | `classify_sites` |
| 41 | `src/backends/rust/item_move/canonical_paths.rs:179` | `nameable` |
| 41 | `src/backends/rust.rs:2096` | `wrap_or_unwrap_return_type` |
| 41 | `src/backends/rust.rs:196` | `client_capabilities` |

## Why deferred

- **The developer's scope for `#reshape`** (2026-10-09): over 60 is in, and 41–60 is deferred.
- **Cost per cut.** Each cut is an `extract_method` plan with `check --deep`, `apply`, clippy and the scoped tests on a
  warm index. 71 plans is a stack's worth of work for "needs attention".
- **Churn.** The crate is split into engine crates plus a wiring package in the next stack
  (`2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md`). Moving these functions first and cutting
  them after keeps the moves' diffs reviewable.

## What would make it worth doing

- A function in the band that a change has to grow: cut it in that change, before adding to it, rather than letting it
  cross 60 and fail the gate.
- Tightening the gate's cap from 60 to 40 is the closing move. It goes in one shrink-only step, with a closed list of
  the functions still over 40, failing when one drops off. That makes this entry a list the test keeps.

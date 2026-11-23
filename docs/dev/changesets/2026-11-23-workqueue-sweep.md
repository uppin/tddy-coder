# 2026-11-23 — `#workqueue` sweep: files the stack left over 500 lines split, dead document writer deleted

**Type:** Architecture

Behaviour-preserving restructure (`#workqueue` 15/15, #1022). No RPC, tool, refusal message, wire shape or
UI changed; every moved body is moved, not rewritten (`restructure verify`: nothing lost).

## Production lines, before → after

| File | Before | After |
|---|---|---|
| `tddy-daemon-rpc/src/work_queue/ports.rs` | 970 | 277 — seven method bodies moved verbatim (hand move, operator-consented) to inherent methods in `branch_reads.rs`, `item_writes.rs`, `repoint_rpc.rs`, `pull_base_rpc.rs` |
| `tddy-daemon-rpc/src/test_util.rs` | 706 | 391 (`test_util/work_queue_service.rs`) |
| `tddy-daemon-rpc/src/exec_tool/work_queue_tools.rs` | 647 | 312 (`work_queue_tools/queue_legs.rs`) |
| `…/work_queue_tools/ops.rs` | 510 | 349 (`ops/item_edits.rs`, `ops/annotation_edits.rs`) |
| `tddy-daemon-rpc/src/exec_tool/work_item_git_tools.rs` | 529 | 290 (`work_item_git_tools/item_git_legs.rs`) |
| `tddy-work-queue/src/rpc.rs` | 514 | 273 (`rpc/service.rs`, re-exported) |
| `tddy-tool-engine/src/work_queue_tools.rs` | 531 | 368 (`work_queue_tools/args.rs`) |
| `tddy-tool-engine/src/lib.rs` | 430 | 451 (`dynamic_proxy.rs`, test module moved out) |
| `tddy-workqueue-web/src/WorkQueueView.tsx` (whole file) | 504 | 489 (`workItemMutationState.ts`) |
| `tddy-worktree-service/src/service.rs` | 897 | 679 — narrowed, still over budget (`service/host.rs`) |

Inline test modules of `land.rs`, `git_ops.rs`, `item_ops/nodes.rs`, `assess.rs`, `changeset/work_queue.rs`, the
tool-engine's `work_queue_tools.rs`, `work_item_git_tools.rs` and `lib.rs`, and daemon-rpc's
`exec_tool/work_queue_tools.rs` moved to `tests.rs`-style child files; the two over 500 (`git_ops`, `nodes`,
changeset) were split again by topic.

## Deleted

`tddy_work_queue::docs::{StackDocsOutput, NodeDocs, validate_stack_docs, write_stack_docs}`, `carries_heading`,
`REQUIRED_CHANGESET_HEADINGS` and their unit tests — no caller outside the crate. `node_doc_paths`,
`NodeDocPaths` and the basenames stay (`tddy-session-files` reads them). Two empty size-budget banners in
`tests/`. Backlog entries resolved: *count-only shape tests* (2026-11-23) and *stack helpers with no
production caller* (2026-11-22).

## Code issues

Closed with the measurements above: `oversized-file-work-queue-ports` (970 → 277), `oversized-file-test-util` (706 → 391),
`oversized-file-exec-tool-work-queue-tools` (647 → 312; `ops.rs` 510 → 349), `oversized-file-exec-tool-work-item-git-tools`
(529 → 290), `oversized-file-rpc` (514 → 273), `oversized-file-work-queue-view` (504 → 489). Narrowed, kept:
`oversized-file-service` (897 → 679; the `impl WorktreeService` trait impl remains).

## Verification

Scoped locally: tool-engine 323/0, changeset 268/0, work-queue + session-files 294/0 (−13 = deleted writer tests),
worktree-service 197 passed (its 12 `remote_git_livekit_acceptance` tests need a binary this worktree did not
build), clippy `--all-targets -D warnings` clean for all six crates. `tddy-daemon-rpc` tests and the web specs were left to CI (not run locally: disk, missing web link).

# 2026-10-02 — Split `plan_store.rs` (522 production lines, budget 500)

**Category:** Deferred (stack rule)
**Source:** `/pr-wrap` file-length gate on #538 (`#live-plan` 2/7)

`packages/tddy-code-restructuring/src/plan_store.rs` was created by #538 at 522 production lines.
Record: `packages/tddy-code-restructuring/docs/code-issues/oversized-file-plan-store.md`.

**Why deferred.** #539 (`live-plans`) extends this file's refresh to every loaded plan; the stack
rule forbids splitting a file a successor also edits. The developer approved the deferral on
2026-10-02.

**What would close it.** Once the `#live-plan` stack has landed, extract the refresh and the flush
policy into `plan_store/refresh.rs` and `plan_store/flush.rs` with `/code-restructuring`, then
re-measure and delete the record.

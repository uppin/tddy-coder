# 2026-11-23 — Armed work items land in wave order and their successors are rebased onto them
**Type:** Feature

`#workqueue` 14/14 ([PR #1015](https://github.com/uppin/tddy-coder/pull/1015)). `tddy_work_queue::engine`
`propagate` and `land` behind `QueueGit` / `QueueGithub` ports: propagation refuses a local branch that is
not clean, pushed and level (naming the state and the actions; `force` never overrides it) and, unless
`force`, a predicted conflict; a land merges a wave range in derived-line order and propagates what sat
on each landed item before the next, stopping at the first item that cannot land. Tools
`work_item_propagate` and `work_queue_land`, RPCs `PropagateWorkItem` and `LandWorkQueue`, the queue
view's **Propagate** and **Land wave**; `work_item_merge` / `work_item_repoint` run through the same
engines, so a merge now rebases its successors. `ScheduleRunner` fires a met schedule once; the daemon
watches each queue with schedules through the GitHub inbox (no polling), acts as the arming user with a
freshly signed token at every use, and re-attaches every armed queue at startup for the `users[]`-mapped
arming user. Landings, propagations, refusals and firings reach the queue's owner as
`WorkQueueChange::{Landed, Propagated, Refused, ScheduleFired}`.

Deleted: `execute_stack_merge` / `execute_stack_repoint` (`merge_repoint.rs`), `recover_in_flight_stack_op`
and the file-journal helpers, `decide_next_action` / `OrchestratorAction` / `AssessTask`, and their three
recipe-era acceptance suites. Resolves `docs/dev/todo/2026-11-22-stack-helpers-with-no-production-caller.md` (on n1's lineage; n14's [F] removes or narrows it)
except `docs::{write_stack_docs, validate_stack_docs}` (the per-PR documents feature).

Packages: tddy-work-queue, tddy-daemon-rpc, tddy-session-activity, tddy-session-lifecycle, tddy-daemon,
tddy-service, tddy-tool-engine, tddy-tools, tddy-agent-launch, tddy-client-web, tddy-workqueue-web,
tddy-workflow-recipes.

Code issues: `complexity-bridge-execute-stack-repoint` closed — the function (last measured 120 lines,
nesting 8) is deleted. New `oversized-file` records for the stack-shared files this PR grew
(`exec_tool/work_item_git_tools.rs` 499 → 529, `exec_tool/work_queue_tools.rs` 547 → 647,
`tddy-work-queue` `rpc.rs` 483 → 514, `WorkQueueView.tsx` 484 → 501); rows added to `work_queue/ports.rs`
(956 → 970) and `test_util.rs` (686 → 706). Splits deferred to the post-stack sweep.

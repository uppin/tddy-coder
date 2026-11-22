# 2026-11-22 — The pr-stack recipe's removal (`#workqueue` 1/14, #1002)

**Type:** Feature (removal)

Receives `internal_status`, `op_journal` (`StackOpJournal`, `MergePhase`, `recover_in_flight_stack_op`), `merge_repoint` (`execute_stack_merge`, `execute_stack_repoint`) and `PR_STACK_PLAN_MD_BASENAME`; loses `stack_ops/seed.rs` and `assess::AssessTask`. Code issue `complexity-merge-repoint-execute-stack-repoint` moved in with the function. Cross-package: [2026-11-22-remove-pr-stack-recipe.md](../../../../docs/dev/changesets/2026-11-22-remove-pr-stack-recipe.md).

# 2026-06-26 — PR-stack orchestration engine: all stubs implemented, sessions can now run end-to-end

- `orchestrate-pr-stack` recipe fully operational: assess → spawn → merge → repoint loop with crash-safe journaling
- `plan-pr-stack` and `orchestrate-pr-stack` added to `--recipe` CLI value_parser (were previously omitted, causing panics)
- `seed_orchestrator_stack_from_plan` bootstraps the Stack DAG on first assess tick from `stack-plan.yaml`
- `RealGithubPrApi`: GET open PRs, PUT merge, PATCH base, POST create — all via curl (no new dependencies)
- `execute_stack_repoint` rebases dependents (`git rebase --onto`), force-pushes, and patches GitHub PR base refs
- Crash recovery: a `PrMerged` journal entry resumes repoint without re-merging on orchestrator restart

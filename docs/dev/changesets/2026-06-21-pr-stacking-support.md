# 2026-06-21 — **PR stacking support

**Type:** Feature

plan-pr-stack and orchestrate-pr-stack recipes** — `tddy-core`: `Stack`/`StackNode` structs, `stack`/`orchestrator_session_id` optional fields on `Changeset`, atomic write helpers, transport-agnostic `spawn_chain_child_worktree`; `tddy-workflow-recipes`: `plan-pr-stack` (analyze-stack→write-stack-plan pipeline, `stack-plan.yaml` artifact) and `orchestrate-pr-stack` (idempotent decision loop, crash-safe `StackOpJournal`, `GithubPrApi`, `git rebase --onto` + `git rerere`); `tddy-coder`: `--stack-parent` and `--stack-base` CLI flags. Feature [pr-stacking.md](../ft/coder/pr-stacking.md). (tddy-core, tddy-workflow-recipes, tddy-coder)

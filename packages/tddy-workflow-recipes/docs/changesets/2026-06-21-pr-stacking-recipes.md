# 2026-06-21 — PR stacking recipes

**Type:** Feature

`plan-pr-stack` recipe (analyze-stack→write-stack-plan→end pipeline, `stack-plan.yaml`+`pr-stack-plan.md` artifacts, `uses_primary_session_document=false`) and `orchestrate-pr-stack` recipe (idempotent `decide_next_action`, crash-safe `StackOpJournal`, `GithubPrApi` trait + `RealGithubPrApi`, `git rebase --onto` + `git rerere`, rollup hooks, `goal_requires_tddy_tools_submit=false`); both registered in `recipe_resolve.rs` and `approval_policy.rs`; `github_rest_common.rs` shared curl helpers.

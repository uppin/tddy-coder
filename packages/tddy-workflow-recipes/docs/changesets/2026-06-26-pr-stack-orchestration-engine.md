# 2026-06-26 — **PR-stack orchestration engine

**Type:** Feature

all stubs implemented** — `github_rest_common.rs`: `curl_github_{patch,post,get,put}_json` (curl temp-file body/response pattern, token-gated); `orchestrate_pr_stack/github.rs`: `RealGithubPrApi` 5 methods (`get_open_pr`, `merge_pr`, `patch_pr_base`, `create_pr`, `disable_auto_merge` best-effort no-op), `owner_repo_from_remote_url` (SSH+HTTPS); `git_ops.rs`: `merge_base`, `rebase_onto` (abort on conflict), `force_push_with_lease`; `assess.rs`: `effective_base_ref`, `assemble_views`, `AssessTask::run`; `bridge.rs`: `seed_orchestrator_stack_from_plan`, `execute_stack_merge` (Planned→PrMerged journal), `execute_stack_repoint` (rebase+push+patch_pr_base per dependent); `actions.rs`: `SpawnTask` (marker file), `MergeTask`, `RepointTask`. Tests: bridge acceptance 3/3, merge+repoint acceptance 2/2. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)

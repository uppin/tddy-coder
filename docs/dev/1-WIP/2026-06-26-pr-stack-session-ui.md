# Changeset: PR-stack session UI — create from new-session screen, display collapsed, finish orchestration engine

**Date:** 2026-06-26  
**Type:** Feature  
**Packages:** tddy-workflow-recipes, tddy-core, tddy-service, tddy-daemon, tddy-coder, tddy-web

## Problem / motivation

PR-stack support landed on 2026-06-21 with the data model (`Stack`/`StackNode`), the `plan-pr-stack` recipe (fully working), and the `orchestrate-pr-stack` recipe skeleton. However:

1. The orchestration engine cannot actually run: `AssessTask`, `SpawnTask`, `MergeTask`, `RepointTask`, all git ops, the real GitHub API, and the plan→orchestrator bridge are all `unimplemented!()` stubs.
2. The web and daemon have zero PR-stack awareness — the child→parent link (`orchestrator_session_id`) is invisible to the API, the new-session screen recipe field is free-text (no PR-stack recipe options), and the session drawer is a flat list with no nesting.

This changeset completes the stack: finishes the engine, surfaces the link through proto/daemon/web, exposes PR-stack recipes in the new-session screen, and renders child sessions collapsed under the orchestrator in the drawer.

## TODO

- [x] Create/update PRD documentation (`docs/ft/coder/pr-stacking.md`)
- [x] Create changeset (`docs/dev/1-WIP/2026-06-26-pr-stack-session-ui.md`)
- [x] **Layer 1 — Finish orchestrate-pr-stack engine** (tddy-workflow-recipes, tddy-core)
  - [x] 1a: `github_rest_common.rs` — `curl_github_patch_json`, `curl_github_post_json`, `curl_github_get_json`, `curl_github_put_json`
  - [x] 1b: `orchestrate_pr_stack/github.rs` — all 5 `RealGithubPrApi` methods; `disable_auto_merge` best-effort log no-op
  - [x] 1c: `orchestrate_pr_stack/git_ops.rs` — `merge_base`, `rebase_onto`, `force_push_with_lease`; `build_integration_ref` left `unimplemented!()` (not needed for linear stacks)
  - [x] 1d: `orchestrate_pr_stack/assess.rs` — `effective_base_ref`, `assemble_views`
  - [x] 1e: `orchestrate_pr_stack/bridge.rs` — `seed_orchestrator_stack_from_plan`, `execute_stack_merge`, `execute_stack_repoint`
  - [x] 1f: `orchestrate_pr_stack/assess.rs` — `AssessTask::run`
  - [x] 1g: `orchestrate_pr_stack/actions.rs` — `SpawnTask::run` (marker file), `MergeTask::run`, `RepointTask::run`
- [x] **Layer 2 — Proto/daemon plumbing** (tddy-service, tddy-daemon)
  - [x] `connection.proto`: `orchestrator_session_id = 21` on `SessionEntry`
  - [x] `session_list_enrichment.rs`: `orchestrator_session_id` on `SessionListStatusDisplay`, populated from changeset
  - [x] `connection_service.rs`: `orchestrator_session_id` applied to proto `SessionEntry` via enrichment
  - [x] TS proto regenerated: `orchestratorSessionId` present in `gen/connection_pb.ts`
- [x] **Layer 3 — New-session screen** (tddy-coder, tddy-daemon, tddy-web)
  - [x] `run.rs:582,775`: `plan-pr-stack`, `orchestrate-pr-stack` in `--recipe` value_parser
  - [x] `connection.proto`: `stack_parent = 15` on `StartSessionRequest`
  - [x] `spawner.rs`: `stack_parent: Option<&str>` on `SpawnOptions`, threads `--stack-parent` arg
  - [x] `connection_service.rs`: `stack_parent_for_spawn` derived from request, passed to `SpawnOptions`
  - [x] `CreateSessionPane.tsx`: recipe `<select>` with all 9 recipes (default "tdd"); parent-picker `<select>` loaded from `listSessions`; `stackParent` passed to `startSession`
  - [x] `stackParents.ts` — `stackParentCandidates(sessions)` helper
- [x] **Layer 4 — Session drawer grouping** (tddy-web)
  - [x] `sessionStackGroups.ts` — `groupSessionsByStack(sessions)` → groups + flat
  - [x] `SessionDrawer.tsx` — `<details>`/`<summary>` groups for stacks, flat for orphans
  - [x] `SessionDrawerItem.tsx` — `depth?: number` with `data-depth` attribute
- [x] Rust tests passing (bridge acceptance: 3/3, merge/repoint acceptance: 2/2, workspace session acceptance, recipe resolver: 6/6 — all green)
- [x] Cypress component tests (`bun run cypress:component` — 23/23 passing across 5 suites)
- [x] Update `session-drawer.md` to document the recipe dropdown + parent picker + drawer grouping

## Acceptance criteria

1. From the new-session screen, creating a "Tool" session presents a recipe dropdown with "Plan PR stack" and "Orchestrate PR stack" options.
2. Selecting "Orchestrate PR stack" and submitting creates a session that runs the orchestrate-pr-stack recipe without panicking.
3. A child session created with `--stack-parent <orch-id>` appears in the session drawer nested (collapsed, depth=1) under the orchestrator session.
4. Clicking the `<details>` group header collapses / expands the child sessions beneath it.
5. The orchestrator's `assess` loop, when given a stack with one planned node, spawns a child `tddy-coder` process with `--stack-parent` and `--stack-base` and records its session_id in `Stack.nodes[].session_id`.
6. After a node's PR is merged, `repoint` updates dependent PRs' base refs via `PATCH /repos/{repo}/pulls/{n}` and force-pushes rebased branches.
7. The `stack-status.md` rollup table reflects current node phases across assess ticks.
8. Crash recovery: restarting the orchestrator session after a `PrMerged` journal entry resumes repointing without re-merging.

## References

- PRD: `docs/ft/coder/pr-stacking.md`
- Session drawer: `docs/ft/web/session-drawer.md`

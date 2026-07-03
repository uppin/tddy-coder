# Changeset: pr-stack-free-prompting-loop — operator-driven PR-stack with PR-management tools + internal statuses

**Date:** 2026-07-03  
**Branch:** `gaudy-newsprint`  
**Packages:** `tddy-core`, `tddy-workflow-recipes`, `tddy-tools`, `tddy-daemon`, `tddy-web`  
**Feature PRD:** [docs/ft/coder/pr-stacking.md](../../ft/coder/pr-stacking.md) — see the 2026-07-03 "free-prompting operator loop" note, [PR-management tools](../../ft/coder/pr-stacking.md#pr-management-tools), and [Internal PR status](../../ft/coder/pr-stacking.md#internal-pr-status).

## Context

The `pr-stack` recipe currently runs an **automatic agentic loop**: after planning it auto-cycles
`assess → spawn / merge / repoint` to drive the whole stack to master with no human turn-by-turn. We
are removing that autopilot. After the plan exists, the same orchestrator session drops into an
**interactive free-prompting chat** (`orchestrate` goal). The agent manages the stack explicitly via
new `tddy-tools` PR-management tools, and each planned PR gains an **internal status** (auto-derived
from git + GitHub, agent-overridable) surfaced in the web UI. The planning phase is unchanged; the
merge/repoint/GitHub bridge logic is retained but is now tool-invoked, not loop-driven.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `StackNode.internal_status: Option<PrInternalStatus>` + `PrInternalStatus { kind, note, source }` (`tddy-core/src/changeset.rs`) + migrate all `StackNode` literals
- [x] `pr-stack` recipe graph: `analyze-stack → write-stack-plan → orchestrate` terminal loop; dropped `begin-orchestrate`/`assess`/`spawn`/`merge`/`repoint` from the graph; `orchestrate` goal hint (`AcceptEdits`, PR tools + `Agent`) (`pr_stack/mod.rs`)
- [x] `PrStackRecipe::next_goal_for_state[_with_changeset]` resumes into `orchestrate` (`pr_stack/mod.rs`)
- [x] `PrStackRecipe::orchestration_system_prompt` override: chat-oriented `orchestrate` prompt (`pr_stack/mod.rs`)
- [~] Legacy `OrchestratePrStackRecipe` graph **left intact** (deviation) — it's only reachable via an alias that resolves to `PrStackRecipe`, so its graph is inert in production, and its tests still cover the retained merge/repoint bridge logic. Flagged for review.
- [x] `close_pr(number)` on `GithubPrApi` trait + `RealGithubPrApi` + test mock (`orchestrate_pr_stack/github.rs`)
- [x] `derive_internal_status` + `reconcile_internal_status` (override-wins), new `orchestrate_pr_stack/internal_status.rs`
- [x] `pr_merge_action` / `pr_close_action` / `pr_resolve_conflicts_action`, new `orchestrate_pr_stack/pr_actions.rs`
- [x] New MCP tools in `PermissionServer`: `pr_stack_status`, `pr_merge`, `pr_repoint`, `pr_close`, `pr_resolve_conflicts`, `pr_set_status`, `pr_add_planned` (7 fully implemented); `pr_spawn_child` (honest stub, see below) (`tddy-tools/src/server.rs`)
- [ ] `pr_spawn_child` daemon routing — **DEFERRED**: implemented as a stub returning a structured "use the web Start-session button" error with `// TODO(pr-spawn-child)`; the daemon `StartSession` relay wiring (async + daemon URL/token env for a local managed orchestrator) is not yet in place. `pr_add_planned` mutates the changeset directly, no daemon needed.
- [x] TS `StackNode.internalStatus` + parse in `parseStackPlan` (`stackPlan.ts`)
- [x] Internal-status badge in `PlannedPrRow.tsx`
- [x] Dead `AssessTask`/`SpawnTask`/`MergeTask`/`RepointTask` no longer wired into the pr-stack graph; helper fns kept

## Validation Results (PR-wrap, 2026-07-03)

Independent diff review + fmt/clippy/test gate. `fmt --check` clean; `clippy -D warnings` clean on `tddy-core`/`tddy-workflow-recipes`/`tddy-tools`; all changed-crate tests pass (the one `session_action_jobs_acceptance` failure is a pre-existing 20ms timing flake — passes 3/3 in isolation — unrelated to this change).

**Findings resolved:**
- **CRITICAL — stack never seeded on a fresh session.** Removing `begin-orchestrate` dropped the only stack-seeding path, so `orchestrate` and every `pr_*` tool ran on an empty stack. Fixed: `after_write_stack_plan` (`pr_stack/hooks.rs`) now always calls `reseed_stack_from_plan_if_unspawned` (validates + seeds from empty + refuses after a spawn). New regression guard: `tests/pr_stack_seeding_acceptance.rs` exercises the actual hook.
- **MEDIUM — `has-conflicts` clobbered on the next status refresh.** `pr_resolve_conflicts` now writes `has-conflicts` as a sticky `override` (survives derivation) and clears it (only when currently `has-conflicts`) once the branch merges cleanly (`server.rs`).
- **MEDIUM — `pr_resolve_conflicts_action` masked a refused merge as "clean".** Now distinguishes a merge that could not start (dirty tree / bad ref → error) from a genuinely clean merge (`pr_actions.rs`); tool description reframed as detect-only.
- **LOW — silent `"main"` default-branch fallback.** `default_branch()` now returns an error (surfaced via `refresh_error`) instead of guessing, per CLAUDE.md's no-silent-fallback rule (`server.rs`).

**Follow-ups (deferred, documented):**
- `pr_spawn_child` daemon `StartSession` relay wiring (`// TODO(pr-spawn-child)`); today returns an honest "use the web Start-session button" error.
- Crash-recovery `recover_in_flight_stack_op` is no longer auto-invoked (was called by the removed `begin-orchestrate`). The tool-driven flow no longer writes `StackOpJournal`, so this is latent; a legacy session mid-merge would strand its journal.
- `pr_stack/bridge.rs` `BeginOrchestrateTask` is now unwired (retained, `pub`, no warning). Remove or repurpose for managed-path seeding in a follow-up.
- Legacy `OrchestratePrStackRecipe` graph left intact (inert; reachable only via an alias that resolves to `PrStackRecipe`; its tests still cover the retained merge/repoint bridge).

## Green status (2026-07-03)

All Rust target tests pass; `cargo clippy` clean on changed crates; `cargo fmt` applied. `tddy-tools` builds with the new tools. **Not verified in this environment (offline, no `node_modules`):** the web `tsc` typecheck and the `PlannedPrRowInternalStatusAcceptance.cy.tsx` Cypress spec — run `./dev bun install && ./dev bun run cypress:component` to close this gap. Pre-existing/environmental failures unrelated to this change: `tddy-daemon` `spawner`/`sandbox_session` tests (need pre-built binaries) and `tddy-integration-tests` `sandbox_egress_relay_tls` (pre-existing `SandboxRunnerArgs` compile error outside this diff).

## Acceptance tests

- [x] `packages/tddy-core/tests/pr_stack_data_model_acceptance.rs` — `internal_status` round-trips through `changeset.yaml`; absent field → `None` (back-compat)
- [x] `packages/tddy-workflow-recipes/tests/pr_stack_free_prompting_acceptance.rs` — `pr-stack` graph ends at a terminal `orchestrate` loop with no auto assess/spawn/merge/repoint edges; `next_goal_for_state_with_changeset` resumes to `orchestrate` when the stack is populated
- [x] `packages/tddy-workflow-recipes/tests/pr_stack_seeding_acceptance.rs` — the `write-stack-plan` hook seeds `Changeset.stack` from the plan on first write (regression guard for the removed `begin-orchestrate` seeder)
- [x] `packages/tddy-workflow-recipes/tests/pr_stack_internal_status_acceptance.rs` — `pr_stack_status` derivation yields `needs-repoint` / `has-conflicts` / `ready-to-merge` from assembled views; an `override` status is preserved across derivation
- [x] `packages/tddy-workflow-recipes/tests/pr_management_actions_acceptance.rs` — `pr_merge_action` / `pr_close_action` call `GithubPrApi` and persist the node phase; `pr_resolve_conflicts_action` reports conflicted paths from a real git fixture
- [x] `packages/tddy-web/cypress/component/PlannedPrRowInternalStatusAcceptance.cy.tsx` — badge renders per `internalStatus.kind`; absent when no internal status (`mountWithRpc`)

## Unit tests

- [ ] `derive_internal_status` decision table (rstest, one case per kind) + override-wins rule (`orchestrate_pr_stack/`)
- [ ] `close_pr` issues `PATCH pulls/{n} {state: "closed"}` (mock transport)
- [ ] Each new tool: happy path + error (missing node, absent `GITHUB_TOKEN`, git failure) (`tddy-tools/src/server.rs` `#[cfg(test)]`)
- [ ] `PrStackRecipe::build_graph` has no `spawn`/`merge`/`repoint` edges; `orchestrate` has no successor (`pr_stack/mod.rs` `#[cfg(test)]`)

## Delta summary

### `tddy-core`
- `changeset.rs` — add `StackNode.internal_status: Option<PrInternalStatus>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) and `struct PrInternalStatus { kind: String, note: Option<String>, source: String }` (`Default`, `Serialize`, `Deserialize`). Orthogonal to `pr_status`.

### `tddy-workflow-recipes`
- `pr_stack/mod.rs` — `build_graph` → `analyze-stack → write-stack-plan → orchestrate` (terminal, no `end` edge, `BackendInvokeTask`); drop `begin-orchestrate`/`assess`/`spawn`/`merge`/`repoint`. Add `orchestrate` `goal_hints` (`AcceptEdits`, allowed tools = PR-management tools + `Agent`), update `goal_ids`, `next_goal_for_state[_with_changeset]` (resume → `orchestrate`), `status_for_state`. Override `orchestration_system_prompt` with a chat-oriented `orchestrate` prompt.
- `orchestrate_pr_stack/github.rs` — add `close_pr(number)` to `GithubPrApi` + `RealGithubPrApi` (`curl_github_patch_json(repo, "pulls/{n}", {"state":"closed"})`) + `MockGithubTransport`.
- `orchestrate_pr_stack/` — new `derive_internal_status` reusing `assemble_views` / `effective_base_ref` / `PrLiveStatus`; override-wins. Remove dead Task impls from the graph (keep `assemble_views`, `effective_base_ref`, `execute_stack_merge`, `execute_stack_repoint`); remove `spawn-request-*.json` writer.
- `orchestrate_pr_stack/mod.rs` — mirror graph-edge removal in legacy `OrchestratePrStackRecipe`.

### `tddy-tools`
- `server.rs` — new MCP tools in the `#[tool_router] impl PermissionServer` block: `pr_stack_status`, `pr_merge`, `pr_repoint`, `pr_close`, `pr_resolve_conflicts`, `pr_set_status`, `pr_add_planned`, `pr_spawn_child`. Each reads/writes the orchestrator changeset via session context; reuses `RealGithubPrApi`, `merge_pr/git_ops.rs`, `pr_stack::add_planned_pr_node`. Auto-allowed by existing `decide()`.

### `tddy-daemon`
- `session_toolcall.rs` / `connection_service.rs` — route `pr_spawn_child` (and `pr_add_planned` if it needs the daemon) through the toolcall relay to the existing `StartSession` / `add_planned_pr_node` path (add a small relay verb if none exists).

### `tddy-web`
- `components/sessions/prstack/stackPlan.ts` — add `internalStatus?: { kind; note?; source }` to TS `StackNode`; parse in `parseStackPlan`.
- `components/sessions/prstack/PlannedPrRow.tsx` — render an internal-status badge (colored by kind, `note` as title, new `data-testid`).

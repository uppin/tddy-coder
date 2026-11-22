# Initial Discovery: remove the pr-stack workflow recipe; stacks live in the project stack view and the stack tools

**Changeset**: [2026-11-22-remove-pr-stack-recipe.md](./2026-11-22-remove-pr-stack-recipe.md)
**Date**: 2026-11-22
**Passes**: Explorations 1–3 copied in full from the `#workqueue` whole-work discovery; node-specific follow-up appended as Exploration 4.

## Combined Conclusions

### What exists today

| Area | State A | Where |
|---|---|---|
| Stack store | YAML records `<data>/projects/<project_id>/stacks/<stack_id>.yaml`, keyed by **project then stack id**; `StackRecord { stack_id, project_id, created_by_session_id, created_at_unix_ms, title, stack }`; many stacks per project; a session points at one through `Changeset.stack_id` | `tddy-changeset/src/project_stack_store.rs`, `changeset/stack.rs` |
| Item model | `StackNode { node_id, title, description, branch_suggestion, branch, session_id, parents, pr_status, child_state, internal_status, display_order }` — no PR number, no labels/badges, no dependency edges apart from `parents`, no associated sessions beyond one `session_id` | `changeset/stack.rs:26-63` |
| Creator recorded | four meanings: session first write (own id + `stack_id`), `pr_stack_create` tool (caller id, no `stack_id`), `CreateProjectStack` RPC (**empty string**), migration (legacy session) | Exploration 3 §4A.3 |
| Migration precedent | `migrate_session_stacks` at daemon start (`last_run_sweep.rs:29-35`), idempotent by `created_by_session_id`; "no dual read" decision | `tddy-pr-stack/src/stack_migration.rs` |
| Agent tools | 17 `pr_*` tools (11 plan/read + 6 git), **gated by session role** (`is_orchestrator_role`) at listing and at execution; a node session gets two reads | `tddy-tool-engine/src/stack_tools.rs`, `stack_git_tools.rs`, `stack_tools/caller_records.rs`, `daemon-rpc/exec_tool/stack_tools.rs` |
| Merge / repoint | `pr_merge` merges and marks merged; dependents are journalled but **never repointed**; `pr_repoint`/`pr_set_parents` rebase + force-with-lease + patch base; propagation exists only as the recipe's `execute_stack_repoint` | `git_work.rs`, `git_work/node_ops.rs`, recipes `orchestrate_pr_stack/bridge.rs` |
| Waves | only `tddy-build/src/graph.rs::waves` (unrelated crate); stack has `topo_order`, `display_order`, `assess::decide_next_action`, `derive_internal_status` | Exploration 3 §4B.5 |
| Git helpers | ahead/behind (`base_sync::count_divergence`), merge-tree prediction (`conflicting_paths`, `merge_tree_conflicts`), sha-keyed cache, `rebase_onto`, `force_push_with_lease`, `merge_base`, `fetch_ref`, first-parent commit listing (`lineage.rs`) | Exploration 3 §4E |
| `gh stack` | **no code calls it** | Exploration 3 §3 |
| GitHub reads | one conditional poller (trunk feed: ETag/304/`X-Poll-Interval`, commits list, per project); everything else is unconditional curl, uncached, headers discarded, no rate-limit handling; `ProjectStackView`/`PrStackScreen` poll `QueryBranch` every 5 s **per rendered branch per browser view** | Exploration 2 §4A |
| Check runs | read only by `read_pr` on an agent call; `CheckRun { name, conclusion }` — no status, id, url, timing | `tddy-github/src/pr_api.rs:141` |
| Inspector | tabs Details/Activity/Tools/Agents/Usage/Worktree/Files/VNC/Screen Sharing; default `details` via `?inspector=`; a **Worktree tab already exists** (`SessionWorktreeTab`: disk, branch, changed-file count, Refresh/Clear/Delete/Restore) — no dirty-file list, no unpushed/unpulled; **no PR tab** | `tddy-session-web/src/InspectorTabs.tsx`, `SessionInspectorDrawer.tsx`, `tddy-workspace-web/src/worktrees/SessionWorktreeTab.tsx`, `tddy-client-web/src/appRoutes.ts:239-286` |
| Session → PR | only `SessionEntry.branch`; no PR number anywhere; PR found by head-branch lookup | Exploration 2 §4B |
| Activities / watches | `SessionNotificationSource` → project notification log → `WatchKind` (8 kinds); adding a kind touches ~12 places (the `TrunkCommit` precedent) | Exploration 3 §4D.2 |
| Recipe | `pr-stack` + aliases `plan-pr-stack`/`orchestrate-pr-stack`; `PrStackScreen`; `write-stack-docs`; `--stack-seed-base-session` chain from web form to CLI; recipe-name lists in 6 places; the **child `stack-progress` hook was never implemented** (doc + todo only) | Exploration 1 §4A |

### Facts that change the brief's premises (reported to the orchestrator)

1. **n1 cannot delete `orchestrate_pr_stack` outright.** `internal_status.rs`, `transient.rs` (`StackOpJournal`, `MergePhase`) and `bridge.rs` (`execute_stack_merge`, `execute_stack_repoint`) live only in `tddy-workflow-recipes` and the daemon's live stack tools import them; `stack_ops` is reached through the recipe's re-export in `daemon-rpc`; `tddy-session-files` imports `PR_STACK_PLAN_MD_BASENAME` from the recipe. n1 moves the survivors into `tddy-pr-stack` (which also narrows the unclaimed `squatting-pr-stack-data-model` code issue).
2. **The `stack-progress` hook does not exist in code** — n1 deletes the doc section and resolves `2026-08-29-stack-progress-json-…`.
3. **Planned-PR row coverage lives mostly in `PrStackScreen` specs** (39 session-web specs drive it through a `recipe: "pr-stack"` session). Deleting the screen deletes that coverage unless the specs are re-pointed at `ProjectStackView` — n1 re-points the ones whose behaviour `ProjectStackView` still has.
4. **Persisted changesets naming the recipe fail resolution** on resume (accepted by the operator) — and must not break **listing**; n1 makes listing tolerate an unknown legacy recipe name.
5. **A Worktree tab already exists.** n4 extends it (dirty files, unpushed/unpulled, actions) rather than introducing it.
6. **The trunk watch reads the commits list, not the events feed — a recorded decision** (`docs/dev/changesets/2026-11-21-trunk-watch.md`, "The commits list, not the events feed"). n2 moving it onto a per-repository events feed reopens that decision; n2 keeps the commits-list read as the trunk watch's source inside the inbox (one cached conditional row per repo+branch) unless the operator says otherwise.
7. **Stack tools are role-gated today**; the operator amendment requires access by queue ownership/addressing. n5 replaces the role gate for the queue tools (the trunk-watch subscription and other role checks stay — out of scope).
8. **Migration has two cases with no single creating session**: `CreateProjectStack`-RPC records carry `created_by_session_id = ""`, and one session can have created several records (`pr_stack_create` is repeatable). n5 must decide where these go — flagged as an operator choice.
9. **No `gh stack` reader exists**, and `gh stack view` reads local tracking only. Line edges from PR base refs are reliable; `gh stack` membership is an extension-dependent extra — n9 treats it as optional input.
10. **`\bStack\b` also names unrelated types in `tddy-code-restructuring`** (109 hits) — n3's rename is symbol-scoped, never textual.
11. **The closed RPC list in `tddy-service/tests/unbundle_service_split.rs`** (`PR_STACK_METHODS`, missing `CreateProjectStack`) and the closed tool lists (`STACK_TOOL_NAMES`, `STACK_GIT_TOOL_NAMES`, `read_only.rs`, `exec_tool_admission.rs`, `stack_tools_catalogue.rs`, `project_home_prompt.md`) move together in n3; every later node that adds an RPC or a `work_*` tool registers it in the same lists.

### Backlog and code issues in the path (none claimed by another stack)

Every relevant `docs/dev/todo/` entry and `code-issues/` record is unclaimed (`squatting-pr-stack-data-model.md` says "— unclaimed"). Per-node verdicts are in each node's `## Prerequisites`. Files at or near the 500-line budget that several nodes touch: `daemon-rpc/src/pr_stack/ports.rs` (880, standing debt), `tddy-changeset/src/changeset/stack.rs` (463), `tddy-tool-engine/src/stack_tools.rs` (463), `tddy-pr-stack/src/git_ops.rs` (468), `daemon-rpc/exec_tool/project_home.rs` (468), `tddy-session-activity/src/project_notification_log.rs` (499 — any new source arm pushes it over).

### Packages by node

| Node | Packages |
|---|---|
| n1 remove-pr-stack-recipe | tddy-workflow-recipes, tddy-pr-stack, tddy-coder, tddy-agent-launch, tddy-agent-backend, tddy-agent-skills, tddy-presenter, tddy-spawn, tddy-supervisor wire, tddy-tool-engine, tddy-tools, tddy-tools-cli, tddy-service (session.proto field 31), tddy-session-files, tddy-session-worktree, tddy-session-activity, tddy-daemon-rpc, tddy-telegram-control, tddy-session-create-web, tddy-session-web, tddy-prstack-web, tddy-web, tddy-storybook, tddy-client-web (gen) |
| n2 github-event-inbox | tddy-github, tddy-session-activity, tddy-session-lifecycle, tddy-daemon, tddy-daemon-rpc (QueryBranch PR leg) |
| n3 workitem-rename | every package carrying the stack vocabulary (Exploration 1 §3 counts) |
| n4 session-pr-tabs | tddy-service (proto), tddy-worktree-service, tddy-daemon-rpc, tddy-workspace-web, tddy-session-web, tddy-client-web |
| n5–n14 | tddy-changeset, tddy-pr-stack, tddy-tool-engine, tddy-daemon-rpc, tddy-session-activity, tddy-daemon-kernel, tddy-service, tddy-prstack-web, tddy-session-web (names after n3's rename) |


## Exploration 1 — the pr-stack recipe and the rename surface

## Discovery dump: removing the `pr-stack` recipe (PR 1) and renaming planned PR / stack to WorkItem / WorkQueue (PR 2)

Repo: `/Users/mantasi/Code/tddy-coder/.worktrees/plan-workqueue-planner`, branch `plan/workqueue-planner`, up to date with `origin/master`. Head commit `97018314f`. Nothing was modified.

---

### 1. Sequence

1. `git status`, `ls packages`, `which rg`. Checked the branch and listed the packages.
2. `rg -l 'pr-stack|pr_stack|PrStack|prStack'` over the whole repo, excluding target, node_modules and .git. This was the broad file inventory (about 640 files) and needed two pages.
3. `rg -n` for `"pr-stack"|plan-pr-stack|orchestrate-pr-stack|write-stack-docs|stack-progress|stack_seed_base_session|stack-seed-base-session|stackSeedBaseSession`, leaving out md, changesets and changelogs. This found the recipe name, aliases, goal, hook and flag in code. The output was saved to disk and read in two parts.
4. `ls` of the recipe module directories, `tddy-pr-stack/src`, `tddy-prstack-web/src` and `tddy-tool-engine/src`, plus `wc -l` of the recipe modules. This gave the module layout.
5. Read `tddy-workflow-recipes/src/pr_stack/mod.rs` lines 1–440 (the recipe definition), `src/lib.rs` (registration and exports) and `src/recipe_resolve.rs` (aliases).
6. Read `approval_policy.rs` lines 1–60, then grepped the `pub` items of `plan_pr_stack`, `orchestrate_pr_stack` and `pr_stack`. This was the recipe-name registry and the public surface.
7. `rg` for every outside consumer of `orchestrate_pr_stack::`, `plan_pr_stack::`, `pr_stack::` and the recipe types and constants. This was the dependency check for deletion.
8. Read the head of `orchestrate_pr_stack/mod.rs`, the imports of `daemon-rpc stack_tools/ops.rs`, `tddy-tools github_credential.rs` and the head of `plan_pr_stack/mod.rs`. These are the shared helpers that outlive the recipe.
9. `rg 'stack[-_]progress|StackProgress'`, then read the stack-progress test, the doc section and the todo entry. This established whether the hook exists.
10. Read `tddy-coder/src/run.rs` around 630–640, 870–882 and 1675–1800, plus the seed-helper grep. This is the CLI side of `--stack-seed-base-session`.
11. `rg` for the seed helpers, then read `agent-launch/stack_seed_validation.rs` and `start_request_checks.rs` lines 450–480. This is the daemon-side validation.
12. `rg 'pr_stack_base_session_id|prStackBaseSessionId|validate_stack_seed_against_project'` and `session.proto` lines 725–745. This is the wire field and its consumers.
13. Read `svc_start_claude_cli_session.rs` lines 30–70, `tool-engine start_options.rs` lines 15–60, and grepped `PR_STACK_RECIPES`. These are recipe-gated wiring points.
14. Read `agent-backend/backend/mod.rs` lines 340–400, `agent-skills/feature_start_slash.rs` lines 1–40, and the presenter consumers. These are the TUI recipe selection and slash menus.
15. Read the web `createSessionRecipes.ts`, `utils/stackParents.ts` and `CreateSessionToolFields.tsx` lines 75–110. This is the recipe dropdown and the "Base the stack on" picker.
16. `rg` for `WORKFLOW_RECIPES|PrStackScreen|prStackOrchestrators|...`, then read `session-web/workflowViews.tsx` lines 1–120. This is the PrStackScreen routing.
17. Read the prstack-web `index.ts`, the imports of each component, `package.json`, and listed `cypress/component`. This is the package boundary.
18. `rg 'from "tddy-prstack-web"'`, then read `attachClaim.ts` lines 30–60 and `session_chain.rs` lines 100–130. These are the importers and the recipe checks outside the recipe.
19. Read `tddy-pr-stack/src/stack_migration.rs` (head and 230–250) and `stack_ops/mod.rs` (1–60 and 260–290). These hold recipe literals inside data-model code.
20. `grep` of service, rpc and message lines in `pr_stack.proto`, then read the proto lines 1–145 and 400–464. This is the proto surface.
21. `rg pr_stack` in `tddy-service` build.rs, `build/*.rs` and `src/lib.rs`. This is the codegen wiring.
22. `rg -l` for `proto::pr_stack|tonic_pr_stack|PrStackService...` over Rust, and `rg -l pr_stack_pb` over TS. These are the generated-code consumers.
23. Read `unbundle_service_split.rs` lines 520–680 (closed RPC-name lists).
24. Read the head of `tool-engine stack_tools.rs` (1–120), `stack_git_tools.rs` (1–80), `stack_tools/defs.rs` (1–60) and `tests/stack_tools_catalogue.rs`. These are where the tool names are registered.
25. `rg -c` of the `pr_*` tool names per file, then read `exec_tool_admission.rs` lines 190–240, `mcp_tool_advertisement_audit.rs` lines 255–290 and the head of `pr_stack_tool_dispatch_acceptance.rs`. These are the remaining tool-name registries.
26. Occurrence counts per package, in code and in markdown, for the three requested term groups.
27. `rg` for Rust `pub struct|enum|trait|type` names containing Stack, PlannedPr or PrStack (the type inventory).
28. Read `tddy-pr-stack/src/lib.rs`, its `Cargo.toml`, and `tddy-changeset/project_stack_store.rs` lines 1–80.
29. Per-package counts for `StackNode`, `ProjectStackStore`, `StackRecord`, `PlannedPr`, `StackPlanOutput`, the handler types, `StackCallerRole` and `Stack`.
30. `rg -o data-testid` over `tddy-prstack-web/src`, plus counts. Then the testid prefixes of chat and dialog, and the test-id helpers under cypress/support.
31. `rg data-testid` with stack or planned outside prstack-web.
32. `wc -l` and headings of `docs/ft/coder/pr-stacking.md` and the other main docs, then read `pr-stacking.md` lines 1–30 and 100–222.
33. `rg -c` of recipe mentions in `docs/ft`, `packages/*/docs` and READMEs.
34. Listed `docs/dev/todo` (475 files). For every entry matching stack or recipe terms, took the title and checked for "Claimed by". Then grepped "Claimed by" across all todo and code-issue files and read `todo/README.md`.
35. Read the category, source and first lines of each relevant todo entry (three batches), and the headers of the code-issue entries.
36. Checked incidental pr-stack mentions in acp, livekit, chat-web and session-activity, and read `session_list_enrichment.rs` lines 75–135.
37. `rg` for recipe gating in `daemon-rpc pr_stack/*`.
38. Every `tddy_workflow_recipes::pr_stack::X`, `orchestrate_pr_stack` and `plan_pr_stack` path outside the recipes crate, plus `PR_STACK_SERVICE`.
39. Read `session-files stack_doc_attachments.rs` lines 15–60 and the imports of `session_context_docs.rs`, then grepped the users of `stack_doc_attachments` and `ChildSpawnHandler`.
40. Read `tddy-spawn/spawner.rs` lines 410–495 and 960–975, and `svc_start_session_core.rs` lines 265–292. This is the spawn plumbing of the seed flag.
41. `rg` for stack fields in `*.proto`, and `supervisor_spawn` / `spawn_worker` (the supervisor JSON wire).
42. Read `tool-engine start_options.rs` lines 1–14 and 135–185, and `start_session_args/schema.rs` lines 148–162.
43. Read `tddy-web/cypress.config.ts` lines 960–985, the head of the e2e spec `pr-stack-chat-response.cy.ts`, and `package.json`.
44. `rg` for the stack goal ids (`analyze-stack`, `write-stack-plan`, `write-stack-docs`, `orchestrate`) outside the recipes crate.
45. Classified each `tddy-workflow-recipes/tests/*stack*` file by its imports.
46. Read the head of `pr_stack_crate_shape.rs` and `tool-engine tests/start_session_schema.rs` lines 48–60 and 128–135.
47. Listed `docs/dev/1-WIP`, read `git log -8`, and grepped `workflow-recipes.md`. Read the 11-line `#workqueue` discovery placeholder.
48. Read `tool-engine read_only.rs` lines 140–175, `project_home_prompt.md` lines 68–78, and the `include_str!` site.
49. Read the stack fields in the Changeset model, the `StackNode` fields, and the stack lines of `session.proto`.
50. Read the `PrStackChat` body and the chat-web testIds.
51. Read the users of `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn` and `link_stack_node_to_child_session`.
52. Counted `pr_*` tool-name mentions in markdown.
53. Read how `pr_stack` is registered as an RPC family: `daemon-rpc families.rs`, `session-lifecycle rpc_families.rs`, `daemon local_socket_server.rs` and the runtime mount files.

---

### 2. Inspected files and the excerpts that mattered

#### The recipe itself (`packages/tddy-workflow-recipes`)

**`src/pr_stack/mod.rs`** (929 lines). This is the recipe defined under the canonical name.
- Lines 23–24 re-export the whole data-model crate under the recipe path. Everything outside that reaches stack operations through `tddy_workflow_recipes::pr_stack::*` depends on this:
  ```
  pub use tddy_pr_stack::docs;
  pub use tddy_pr_stack::stack_ops::*;
  ```
- `STATE_STACK_PLANNED = "StackPlanned"` at :57 and `STATE_STACK_DOCS_WRITTEN = "StackDocsWritten"` at :68. Both are persisted state strings.
- `PR_STACK_TOOL_NAMES` at :71–87 holds 15 `mcp__tddy-tools__pr_*` names. It is the `--allowedTools` list for the `orchestrate` goal, and it does not include `pr_stack_list` or `pr_stack_create`.
- `PR_STACK_ORCHESTRATE_PROMPT` at :91–129.
- `pub struct PrStackRecipe` at :134 with `fn name() -> "pr-stack"` at :138. The graph is `analyze-stack → write-stack-plan → write-stack-docs → orchestrate` (`GraphBuilder::new("pr_stack")`, :175–183).
- `impl SessionArtifactManifest for PrStackRecipe` at :327–385 (stack_plan, stack_plan_md, stack_status_md, stack_status_json, exploration).
- `pub fn reseed_stack_from_plan_if_unspawned` at :398.

**`src/pr_stack/hooks.rs`** (866 lines).
- `RECIPE_NAME = "pr-stack"` at :33.
- The `write-stack-docs` before/after hooks at :204, :308, :359 and :386.

**`src/pr_stack/bridge.rs`** (83 lines). Defines `BeginOrchestrateTask`.

**`src/plan_pr_stack/{mod.rs 529, hooks.rs 376, prompt.rs 242}`**. This is the legacy plan recipe, but it also holds shared parser types:
- `pub struct StackPlanOutput` (:30) and `pub struct PlannedPr` (:41).
- `planned_prs_into_stack_nodes` (:60) and `validate_stack_plan` (:80).
- `pub struct PlanPrStackRecipe` (:159).
- `prompt.rs` exports `write_stack_docs_system_prompt`, `PR_STACK_PLAN_MD_BASENAME` and `STACK_PLAN_BASENAME`.

**`src/orchestrate_pr_stack/*`** (about 1,060 lines in total). The recipe is inert (mod.rs :54–63: "Retained but **inert in production** … kept because its acceptance tests … exercise … helpers reused by the free-prompting `pr_*` tools"). The module re-exports `tddy_pr_stack::{assess, git_ops, pr_insight}` and owns:
- `github.rs`, which is `pub use tddy_github::pr_api::*`
- `bridge.rs`: `execute_stack_merge`, `execute_stack_repoint`, `seed_orchestrator_stack_from_plan`
- `transient.rs`: `StackOpJournal`, `MergePhase`, `recover_in_flight_stack_op`
- `internal_status.rs`: `derive_internal_status`, `reconcile_internal_status`
- `pr_actions.rs`
- `STACK_STATUS_{MD,JSON}_BASENAME`

**`src/lib.rs`**.
- Declares `pub mod orchestrate_pr_stack; pub mod plan_pr_stack; pub mod pr_stack;` (:12, :15, :16).
- Re-exports (:40–64) `OrchestratePrStackRecipe`, `PlanPrStackRecipe`, `PlannedPr`, `StackPlanOutput`, `PrStackRecipe` and `reseed_stack_from_plan_if_unspawned`.
- Re-exports `pub use tddy_pr_stack::PR_STACK_SERVICE;` (:88). A unit test asserts the value `"pr_stack.PrStackService"` (:94).

**`src/recipe_resolve.rs`**:
```
110        "pr-stack" | "plan-pr-stack" | "orchestrate-pr-stack" => { ... let r = PrStackRecipe;
```
Alias tests are at :188–216.

**`src/approval_policy.rs`**. `supported_workflow_recipe_cli_names()` lists `"pr-stack","plan-pr-stack","orchestrate-pr-stack"` (:16–18 and :29–31). `recipe_should_skip_session_document_approval` lists them at :43–45. Tests are at :64–83.

**`proto/write_stack_plan.proto`, `proto/write_stack_docs.proto`, `generated/tdd/write-stack-{plan,docs}.schema.json`, `generated/goal_registry.rs:18-19`, `generated/schema-manifest.json:79-86`**. These are the goal schemas registered for the recipe.

#### Recipe-name registries outside the recipe

- **`tddy-agent-skills/src/feature_start_slash.rs:11-22`**. `SHIPPED_WORKFLOW_RECIPE_CLI_NAMES` includes the three names, with the comment "keep in sync with `approval_policy::supported_workflow_recipe_cli_names`". It drives the `/start-<recipe>` slash labels.
- **`tddy-agent-backend/src/backend/mod.rs:366-372, 388-389`**. `workflow_recipe_selection_question()` has the options "Plan PR stack" and "Orchestrate PR stack". `recipe_cli_name_from_selection_label` maps them to `plan-pr-stack` and `orchestrate-pr-stack`. Its consumer is `tddy-presenter/src/presenter/presenter_impl/backend_selection.rs:142, 176`.
- **`tddy-coder/src/run.rs`**.
  - `--recipe` `value_parser` lists `"pr-stack","plan-pr-stack","orchestrate-pr-stack"` at :814 and :1025.
  - An alias test is at :4809–4815.
- **`tddy-tool-engine/src/start_options.rs:29`**. `pub const PR_STACK_RECIPES: &[&str] = &["pr-stack", "orchestrate-pr-stack", "plan-pr-stack"];` It is used by `pr_stack_orchestrators()` at :152, which mirrors the web.
- **`tddy-session-create-web/src/utils/stackParents.ts:31`**. `export const PR_STACK_RECIPES = [...]` is used by `prStackOrchestrators()` at :39–45.
- **`tddy-session-create-web/src/createSessionRecipes.ts`**. `WORKFLOW_RECIPES` has 8 entries and ends with `"pr-stack"`. This is the **web recipe dropdown**, used by `CreateSessionToolFields.tsx:74` and `CreateSessionManagedCodebaseFields.tsx:65`.

#### Recipe-string checks outside the recipe (these break or go dead on deletion)

- **`tddy-agent-launch/src/svc_start_claude_cli_session.rs:44-66`**. `managed_spawn_handlers` binds `StackChildSpawnHandler` only when `r.name() == "pr-stack"`. The todo `2026-11-14-retire-stack-child-spawn-handler.md` says this handler is unreachable from any tool.
- **`tddy-session-web/src/attachClaim.ts:50`**. `if (session.recipe === "pr-stack") return true;` sits inside `sessionPaneIsWorkflowView`.
- **`tddy-session-web/src/workflowViews.tsx:106-108`**. `if (session.recipe === "pr-stack") { return <PrStackScreen ...` Every other tool recipe gets `WorkflowChatScreen`.
- **`tddy-session-create-web/src/CreateSessionToolFields.tsx:86-108`**. The "Base the stack on" `<select data-testid="create-session-pr-stack-base-session-select">` is shown only when `recipe === "pr-stack"`.
- **`tddy-session-create-web/src/createSessionRequest.ts:191`**. `prStackBaseSessionId: recipe === "pr-stack" ? prStackBaseSessionId : ""`.
- **`tddy-session-worktree/src/session_chain.rs:113-120`**:
  ```
  pub fn parent_is_pr_stack_orchestrator(...) -> bool {
      Ok(cs) => cs.recipe.as_deref() == Some("pr-stack") || cs.stack_id.is_some(),
  ```
  `pr_stack_node_for_spawn` (:140) and `resolve_chain_base_for_session_spawn` (:198) use it. `stack_id` alone keeps the predicate working once the recipe is gone.
- **`tddy-agent-launch/src/stack_seed_validation.rs:38-46`**. It resolves the recipe and requires `r.name() == "pr-stack"`, otherwise `invalid_argument("pr_stack_base_session_id is only supported for the pr-stack recipe…")`.
- **`tddy-coder/src/run.rs:1748`**. `if recipe.name() != "pr-stack" { bail!("--stack-seed-base-session is only supported for the `pr-stack` recipe…`.
- **`tddy-tool-engine/src/stack_tools.rs:117-119`**: "No recipe: the pr-stack recipe is deprecated and never decides a role." The stack tools are role-gated, not recipe-gated.
- **`tddy-daemon-rpc/src/pr_stack/guards.rs:5-7`** confirms it: "(The recipe guard that stood here was deleted in `#orchestrators` 12/12, B9 …)".

#### The `stack-progress` hook

- `docs/ft/coder/pr-stacking.md:309-323` (§ Progress tracking contract) says each child writes `artifacts/stack-progress.json` from "a shared child hook's `after_task`".
- `docs/dev/todo/2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md`: "No production code writes that file; the only repo-wide match is a scratch-directory label…"
- `tddy-workflow-engine/tests/stack_progress_contract_acceptance.rs:16` contains only the scratch label `"tddy-stack-progress-acc-{}-{}"`. The test exercises `sync_stack_node_from_child` and `update_stack_atomic` from `tddy_changeset`, not a hook.

#### `--stack-seed-base-session` plumbing, from the web form to the CLI

- **Web.** `useCreateSessionStack.ts:43, 64, 79` (state plus `stackBaseSessionCandidates`) → `CreateSessionToolFields.tsx:86-108` → `useCreateSessionPaneState.tsx:282, 341, 437` → `CreateSessionPane.tsx:117, 246` → `createSessionRequest.ts:75, 139, 191`.
- **Proto.** `session.proto:737-740`, `string pr_stack_base_session_id = 31;` (comment: "Refused unless `recipe` resolves to "pr-stack"").
- **MCP StartSession args.** `tool-engine/start_session_args.rs:52, 269` and `start_session_args/schema.rs:155` (description: "With the `pr-stack` recipe: an existing session whose branch seeds the new stack…"). `start_options.rs` has `pr_stack_base_candidates` (:144) and `stack_base_session_candidates` (:163).
- **Daemon pre-spawn gate.** `agent-launch/svc_start_session_core.rs:272-288` → `start_request_checks.rs:456-472 validate_stack_seed_against_project` → `stack_seed_validation.rs:27 validate_stack_seed_base_session`, which calls `tddy_workflow_recipes::pr_stack::check_stack_seed_base` and `StackSeedBaseRefusal`.
- **Spawn.** `agent-launch/tool_session_spawn.rs:67, 155` (`trim_to_option(&req.pr_stack_base_session_id)`) → `tool_spawn_plan.rs:48, 69` → `tddy-spawn/spawner.rs:421-423` (`SpawnOptions.stack_seed_base_session`) → `pr_stack_spawn_args(...)` at :473-490, which emits `("--stack-seed-base-session", …)` → call site :966-974. Supervisor and worker JSON go through `spawn_worker.rs:79-81, 266, 367` and `supervisor_spawn.rs:122`.
- **Places that pass `None` or empty.** `agent-launch/svc_resume_session.rs:208`, `project_home_start_spec.rs:142`, `telegram-control/workflow_spawn.rs:349`, `tddy-coder/src/config.rs` (5 sites), `run.rs` (6 test sites).
- **CLI.** `tddy-coder/src/run.rs`:
  - `Args.stack_seed_base_session` (:637)
  - `CoderArgs` `#[arg(long, value_name = "id")]` (:876-878) and the mapping at :1120
  - the call at :1381, `seed_pr_stack_from_base_session_if_requested(args)?;`
  - `seed_stack_and_enter_orchestrate` (:1690), which writes `STATE_STACK_DOCS_WRITTEN`
  - `seed_pr_stack_from_base_session_if_requested` (:1738-1779), which calls `pr_stack::check_stack_seed_not_self`, `check_stack_seed_base` and `seed_stack_with_base_session`
- **Writer.** `tddy-pr-stack/src/stack_ops/seed.rs`: `check_stack_seed_base` (:80), `check_stack_seed_not_self` (:128), `seed_stack_with_base_session` (:162), `StackSeedBaseRefusal` (:34), re-exported at `stack_ops/mod.rs:35-37`.

#### `tddy-prstack-web` (web)

- **`src/index.ts`** exports `PrStackScreen`, `PrStackChat`, `PlannedPrList`, `ProjectStackView`, `PlannedPrPanel`, `WorkflowChatScreen` and `StackChildMetadata`.
- **Who imports each internal module** (non-test):

  | Module | Imported by |
  |---|---|
  | `PlannedPrPanel` | index, PrStackScreen; storybook |
  | `PrStackChat` | index, PrStackScreen; cypress tests in session-web and tddy-web |
  | `PrStackStartSessionDialog`, `usePrStackStartSession`, `usePrStackPlan`, `usePrStackBranchMutations` | PrStackScreen only |
  | `stackDocAttachments` | usePrStackStartSession, PrStackScreen |
  | `startBlockers`, `DirtyWorktreeDialog`, `AddPlannedPrForm`, `hydrateStackNodes`, `stackChildSessions` | shared with `ProjectStackView` |
  | `WorkflowChatScreen` | index only; serves every non-pr-stack tool recipe, so it stays |

- **Importers of the package.**
  - `session-web/src/workflowViews.tsx:13` (`PrStackScreen`, `WorkflowChatScreen`)
  - `SessionsDrawerScreen.tsx:31` and `SessionsDrawerDetailPane.tsx:6` (`ProjectStackView`)
  - `useSessionsDrawerScreenActions.ts:17`
  - `tddy-storybook/src/tddy-prstack-web/PlannedPrPanel.stories.tsx:2`
- **Package dependencies.** `package.json` depends on `tddy-chat-web`, `tddy-client-web`, `tddy-react-commons`, `tddy-session-create-web` and `tddy-ui-web`.

#### Proto and generated code

- **`tddy-service/proto/pr_stack.proto`** (464 lines). `package pr_stack;` (:3), `import "types.proto"` (:5), `service PrStackService` (:16). The header comment (:9-12) is stale: it says the service is served from `tddy-workflow-recipes`.
- **Codegen.** `build.rs:65, 86`, `build/unbundled_families.rs:181, 206-207`, `build/descriptors.rs:66`, `src/lib.rs:62` (`pub use proto::pr_stack::PrStackServiceServer`), `:218-219` (`pub mod pr_stack`) and `:235-237` (`pub mod tonic_pr_stack`).
- **TS generated copies.** `tddy-client-web/src/gen/pr_stack_pb.ts` and `tddy-rust-typescript-tests/gen/pr_stack_pb.ts`.

#### Tool-name registries

- **`tddy-tool-engine/src/stack_tools.rs:45-73`**. 11 `pub const PR_*` names plus `STACK_TOOL_NAMES: [&str; 11]`. Also `is_stack_tool`, `is_stack_read_tool`, `StackCallerRole { Orchestrator{own_stack_id}, StackNode{stack_id} }` (:89), `StackCaller`, `ParentStack`, `StackCallerFacts`, `StackScope`, `StackOp`, `StackToolPort`.
- **`stack_tools/defs.rs`**. `stack_tool_defs()` holds the schema descriptions (for example "List the PR stacks of this project…", "add planned PRs to it with pr_add_planned").
- **`tddy-tool-engine/src/stack_git_tools.rs:32-47`**. 6 consts plus `STACK_GIT_TOOL_NAMES: [&str; 6]`, `StackGitOp` (:74) and `StackGitToolPort` (:108).
- **`tddy-tool-engine/src/read_only.rs:28-29`** (`is_stack_tool || is_stack_git_tool`) and **:152-168**, which list all 17 `mcp__tddy-tools__pr_*` names.
- **`tddy-tools/src/exec_tool_admission.rs:211-227`**. A closed list of all 17 bare names. `stack_tool_defs` and `stack_git_tool_defs` are imported at :31-32.
- **`tddy-agent-launch/src/project_home_prompt.md:72-75`** is the agent prompt and names all 17 tools. It is compiled in through `include_str!` at `project_home_launch.rs:201`.
- **Tests that pin the names.**
  - `tool-engine/tests/stack_tools_catalogue.rs:21-33` (`EVERY_STACK_TOOL: [&str; 11]`)
  - `tddy-tools/tests/pr_stack_tool_dispatch_acceptance.rs:29` (uses `tddy_workflow_recipes::pr_stack::PR_STACK_TOOL_NAMES`)
  - `tddy-tools-cli/tests/mcp_tool_advertisement_audit.rs:262-281` (comment names `pr_stack_list` and `pr_stack_create`)
  - `session-lifecycle/connection_service/svc_list_exec_tools_wiring.rs:250` (`vec!["pr_stack_status","pr_read"]`)
  - `tddy-tools/src/server.rs:2556-2565`
  - `stack_tools/caller_records.rs:405`
  - `daemon-rpc/tests/stack_plan_tools_acceptance.rs` (33 occurrences) and `stack_git_tools_acceptance.rs` (19)

#### Closed RPC-name list

**`tddy-service/tests/unbundle_service_split.rs`**:
- `PR_STACK_METHODS: [&str; 10]` at :548-559 holds AddPlannedPr, GetPrStatus, RepointPlannedPr, ReorderPlannedPr, PullBaseIntoBranch, QueryBranch, ResolveStackBase, LinkStackNode, ListProjectStacks and GetProjectStack. **CreateProjectStack is missing**, so the list is not exhaustive. It is only checked as "contains".
- The test `pr_stack_service_declares_every_stack_method` (:627-634) does `service_block(&read("pr_stack.proto"), "PrStackService")`, with the message `"pr_stack.PrStackService is missing {method}"`.
- `RESIDUAL_METHODS` (:578-603) contains no stack RPCs.
- Line 276 says "both ListSessions and pr_stack.QueryBranch reach it".

#### Daemon wiring of the `pr_stack` RPC family

- `tddy-pr-stack/src/rpc.rs:25` defines `pub const PR_STACK_SERVICE: &str = "pr_stack.PrStackService";`. `PrStackHandler` trait (:29), `PrStackServiceImpl<H>` (:92), entry builder (:183-194).
- `tddy-session-lifecycle/src/pr_stack_rpc.rs` re-exports them. `rpc_families.rs:23, 30, 98` (`fn pr_stack_handler`).
- `tddy-daemon-rpc/src/families.rs:22, 25, 94, 119, 191-211` (`pr_stack_service()`, `build_pr_stack_entry`).
- `tddy-daemon/src/local_socket_server.rs:36-37, 54, 120, 158, 220` and `runtime/mount_socket.rs:87-88`, `runtime/tasks.rs:138`.
- `tddy-daemon-rpc/tests/support/mod.rs:56, 185-206` matches on `(PR_STACK_SERVICE, "ListProjectStacks" | "GetProjectStack" | "CreateProjectStack" | "AddPlannedPr")`.

#### Data model

- **`tddy-changeset/src/changeset/stack.rs`**. `pub struct Stack { version, nodes: Vec<StackNode> }` (:18). `pub struct StackNode` (:27), with fields node_id, title, description, branch_suggestion, branch, session_id, parents, pr_status, child_state, internal_status and display_order. `link_stack_node_to_child_session` (:379).
- **`changeset/model.rs`**. `pub stack: Option<Stack>` (:90, legacy, read only by the sweep), `pub stack_id: Option<String>` (:94), `pub orchestrator_session_id` (:99), `GithubPrStatus` (:219), `PrInternalStatus` (:233).
- **`tddy-changeset/src/project_stack_store.rs`**. The header says "PR stacks as project records: `<projects dir>/<project_id>/stacks/<stack_id>.yaml`". `StackRecord` (:19) has a `title` documented "as `pr_stack_create` named it". `StackStoreError` (:36). `ProjectStackStore` (:64). The on-disk directory name `stacks/` is persisted.
- **`tddy-pr-stack/src/lib.rs`**. Modules `assess`, `docs`, `git_ops`, `pr_insight`, `rpc`, `stack_migration` and `stack_ops`.
- **`stack_ops/mod.rs`**. `StackHome` trait (:45), `StoredStack` (:85), and re-exports of `add_planned_pr_node`, `delete_planned_pr_node`, `update_planned_pr_node`, `set_stack_node_parents`, `AddPlannedPrInput`, `UpdatePlannedPrInput`, `move_planned_pr_node`, `assign_missing_display_order`, `pull_base_into_node_branch`, `BaseSyncStrategy`, `repoint_planned_pr_node`, the seed functions and the adopt functions.
- **`tddy-pr-stack/src/stack_migration.rs`** is the startup sweep. It moves `Changeset.stack` into `ProjectStackStore` and is called from `tddy-daemon/src/runtime/last_run_sweep.rs:31`. Its test fixtures hard-code `recipe: Some("pr-stack")` (:244, :356).

---

### 3. Grep and glob log with notable hits

| Pattern | Scope | Notable hits |
|---|---|---|
| `pr-stack\|pr_stack\|PrStack\|prStack` (-l) | whole repo | About 640 files. About 30 are `.agents/` (developer workflow skills about **GitHub PR stacks**, not the product; see Findings). The rest are docs/dev/changesets, docs/ft, and packages listed in §2 |
| recipe, alias, goal, flag literals | code, non-md | §2 plus 39 session-web cypress specs with `recipe: "pr-stack"`, 5 in prstack-web, 4 in tddy-web, and fixtures `stack-parent-candidates.json:5,6,8,12` |
| `stack[-_]progress` | packages, docs | Doc text, the todo, and the scratch label only. **No hook code** |
| `pr_stack_base_session_id\|prStackBaseSessionId` | packages | 12 web sites, 6 Rust prod sites, tests listed in §2 |
| `PR_STACK_RECIPES` | packages | `tool-engine/start_options.rs:29,152`; `session-create-web/utils/stackParents.ts:31,42` |
| `tddy_workflow_recipes::pr_stack::X` outside the crate | rs | agent-launch `stack_seed_validation` (check_stack_seed_base, StackSeedBaseRefusal); coder `run.rs` (STATE_*, check_*, seed_*); daemon-rpc `ports.rs` (add_planned_pr_node, AddPlannedPrInput, BaseSyncStrategy, move_planned_pr_node, pull_base_into_node_branch, repoint_planned_pr_node); `project_stack_queries.rs` (assign_missing_display_order); pr-stack `nodes.rs` (doc ref); tddy-tools test (PR_STACK_TOOL_NAMES) |
| `use tddy_workflow_recipes::(orchestrate_pr_stack\|plan_pr_stack\|pr_stack)` outside | rs | daemon-rpc `pr_status.rs:27,69,188`; `exec_tool/stack_tools/ops.rs:20` (derive/reconcile_internal_status); `stack_git_tools/git_work/node_ops.rs:18` (MergePhase, StackOpJournal); `git_work.rs:420`; tddy-tools `github_credential.rs:16` (RealGithubPrApi, cfg(test)); session-files `stack_doc_attachments.rs:21-22` (PR_STACK_PLAN_MD_BASENAME, NODE_* consts); `session_context_docs.rs:25`; tddy-tools-cli test `stack_docs_submit_contract_acceptance.rs:15` |
| `tddy_workflow_recipes::PR_STACK_SERVICE` | rs | daemon-rpc `ports.rs:314,393`; `project_stack_queries.rs:174`; `project_stacks.rs:279` |
| `proto::pr_stack\|tonic_pr_stack\|PrStackService*` (-l) | rs | 32 files: agent-launch(1), daemon-rpc(19), daemon(6), pr-stack(2), service(2), session-lifecycle(1), workflow-recipes(1) |
| `pr_stack_pb` (-l) | ts/tsx, excluding gen | 49 files: prstack-web(19), session-web(30, of which 21 are cypress) |
| `pr_*` tool names (-c) | packages, non-md | Top files: daemon-rpc `stack_plan_tools_acceptance.rs` 33, tool-engine `stack_tools.rs` 31, `stack_git_tools.rs` 25, daemon-rpc `stack_git_tools_acceptance.rs` 19, recipes `pr_stack/mod.rs` 18, tddy-tools `exec_tool_admission.rs` 17, daemon-rpc `project_home_session_tools_acceptance.rs` 17, `stack_tools_catalogue.rs` 12 |
| `data-testid` stack or planned outside prstack-web | tsx | `session-web/ProjectStacksRow.tsx:42,54` (`sessions-drawer-project-stacks-${projectId}`, `sessions-drawer-project-stack-${stackId}`); `SessionDrawerGroups.tsx:70` (`sessions-drawer-stack-…`); `session-create-web CreateSessionToolFields.tsx:93` (`create-session-pr-stack-base-session-select`); `CreateSessionStackParentSelect.tsx:31` (`create-session-stack-parent-select`); `SessionRuntime.tsx:297` and `DormantSessionRuntime.tsx:184` (`sessions-terminal-pane-stack-…`, which is unrelated: it is a z-stack) |
| `Claimed by` | todo and code-issues | 8 files. The only relevant one is `squatting-pr-stack-data-model.md:9` ("— unclaimed") |

#### Occurrence counts per package

**Code (excludes `*.md`):**

| Package | `PlannedPr\|planned_pr\|plannedPr` | `ProjectStack\|project_stack\|projectStack` | `pr_stack\|PrStack\|prStack` |
|---|---|---|---|
| tddy-session-web | 205 / 17 files | 69 / 8 | 1123 / 57 |
| tddy-prstack-web | 182 / 20 | 41 / 7 | 360 / 33 |
| tddy-pr-stack | 107 / 8 | 38 / 4 | 31 / 8 |
| tddy-daemon-rpc | 106 / 13 | 144 / 16 | 213 / 31 |
| tddy-workflow-recipes | 104 / 13 | 3 / 1 | 210 / 43 |
| tddy-web | 70 / 2 | 15 / 2 | 219 / 9 |
| tddy-client-web (gen) | 42 / 1 | 56 / 2 | 164 / 2 |
| tddy-rust-typescript-tests (gen) | 42 / 1 | 52 / 1 | 164 / 2 |
| tddy-service | 12 / 2 | 15 / 2 | 23 / 8 |
| tddy-storybook | 7 / 1 | – | – |
| tddy-coder | 3 / 1 | – | 17 / 2 |
| tddy-session-lifecycle | 1 / 1 | – | 61 / 15 |
| tddy-session-files | 1 / 1 | – | 6 / 3 |
| tddy-changeset | – | 17 / 5 | 1 / 1 |
| tddy-tool-engine | – | 5 / 2 | 44 / 11 |
| tddy-session-activity | – | 3 / 1 | 2 / 2 |
| tddy-daemon | – | 2 / 1 | 23 / 8 |
| tddy-session-create-web | – | – | 72 / 17 |
| tddy-agent-launch | – | – | 22 / 10 |
| tddy-session-worktree | – | – | 12 / 4 |
| tddy-tools | – | – | 11 / 4 |
| tddy-chat-web | – | – | 8 / 6 |
| tddy-tools-cli, tddy-spawn, tddy-github | – | – | 2 each |

**Markdown:**
- `PlannedPr…`: docs/ft 47, docs/dev 21, tddy-web docs 11, recipes 11, daemon 11, daemon-rpc 10, pr-stack 10.
- `ProjectStack…`: docs/dev 14, docs/ft 13, daemon-rpc 8.
- `pr_stack…`: docs/dev 104 (52 files), recipes 81, docs/ft 80, pr-stack 52, tddy-web 27, daemon-rpc 23, daemon 20.
- `pr_*` tool names in markdown: docs/ft 56, docs/dev 15, plus small counts elsewhere.

**Key type usage** (rs/ts, excluding gen):
- `StackNode`: prstack-web 88, changeset 71, recipes 64, pr-stack 61, daemon-rpc 59, session-lifecycle 21, tool-engine 18.
- `ProjectStackStore`: daemon-rpc 29, changeset 16, pr-stack 13.
- `StackRecord`: daemon-rpc 19, changeset 11, tool-engine 7.
- `PlannedPr` (the Rust struct): recipes 38.
- `StackPlanOutput`: recipes 39.
- `PrStackRpcHandler|PrStackHandler|PrStackServiceImpl`: daemon-rpc 39, session-lifecycle 10.
- `StackCaller*`: tool-engine 70, daemon-rpc 18.
- `\bStack\b` also matches 109 in `tddy-code-restructuring`, which is unrelated. A blind rename would break it.

---

### 4. Findings

#### A. The `pr-stack` workflow recipe

**A1. Definition and registration** (all in `tddy-workflow-recipes`)
- `PrStackRecipe` is defined at `src/pr_stack/mod.rs:134`. Its hooks are `PrStackHooks` (`hooks.rs`) and `BeginOrchestrateTask` (`bridge.rs`).
- It is resolved in `recipe_resolve.rs:110` and listed in `approval_policy.rs:5-53`.
- Two legacy recipes are still compiled: `PlanPrStackRecipe` (`plan_pr_stack/mod.rs:159`) and `OrchestratePrStackRecipe` (`orchestrate_pr_stack/mod.rs:64`, documented as inert).
- Goal schemas: `proto/write_stack_{plan,docs}.proto`, `generated/tdd/write-stack-{plan,docs}.schema.json`, `goal_registry.rs:18-19`, `schema-manifest.json:79-86`.
- Recipe-name lists outside the crate: `tddy-agent-skills feature_start_slash.rs:11-22`, `tddy-agent-backend backend/mod.rs:366-389` (TUI `/recipe` picker labels "Plan PR stack" and "Orchestrate PR stack"), `tddy-coder run.rs:814,1025` (clap value_parser), `tool-engine start_options.rs:29`, web `createSessionRecipes.ts` and `stackParents.ts:31`.

**A2. Aliases.** `plan-pr-stack` and `orchestrate-pr-stack` resolve to `PrStackRecipe` (`recipe_resolve.rs:110`). They are tested at `recipe_resolve.rs:188-216`, `approval_policy.rs` tests, `run.rs:4809-4815`, `add_planned_pr_unit_tests.rs:236-261`, `session_list_enrichment.rs:1445-1499, 1656-1673` (persisted legacy recipe strings), `prStackOrchestrators.test.ts`, `CreateSessionPane.cy.tsx:548-622` and `stack-parent-candidates.json`.

**A3. `write-stack-docs`.**
- The goal is defined in `pr_stack/mod.rs:155-163, 210-218, 241, 256`. Its hooks are in `pr_stack/hooks.rs:204-390`, and its prompt is `plan_pr_stack/prompt.rs:138-215` (`write_stack_docs_system_prompt`).
- The payload type and validator live in **`tddy-pr-stack/src/docs.rs`** (`StackDocsOutput`, `node_doc_paths`, `validate_stack_docs`, `NODE_*` consts). These are re-exported through `pr_stack::docs`.
- Tests: `recipes/tests/pr_stack_docs_acceptance.rs`, `pr_stack_docs_validation_acceptance.rs`, `tddy-tools-cli/tests/stack_docs_submit_contract_acceptance.rs` and `cli_integration.rs:25-26, 47-48` (closed goal list).
- **Shared helpers outside the recipe use `NODE_PRD_BASENAME`, `NODE_CHANGESET_BASENAME`, `NODE_DOCS_SUBDIR` and `PR_STACK_PLAN_MD_BASENAME`.** They are imported by path from `tddy_workflow_recipes::{pr_stack, plan_pr_stack}` in `tddy-session-files/src/stack_doc_attachments.rs:21-22` and `session_context_docs.rs:25`.
  - `stack_doc_attachments` is called in production by `agent-launch/child_spawn_handler.rs:42` (the stack child spawn handler).
  - `prompt_with_attached_changeset` from the same file is also used by `tddy-session-split/src/attached_initial_prompt.rs:38`, which is not part of the recipe.

**A4. The child `stack-progress` hook does not exist in code.** Only docs describe it (`docs/ft/coder/pr-stacking.md:309-323`), plus the todo `2026-08-29-stack-progress-json…` and a test scratch label. Deleting it means deleting doc text and the todo. The orchestrator-side rollup is real, though: `OrchestratePrStackHooks::after_task` in `orchestrate_pr_stack/hooks.rs` writes `stack-status.{md,json}`, and the `STACK_STATUS_*` consts are in `PrStackRecipe`'s manifest.

**A5. `--stack-seed-base-session`.** The full chain is in §2. Every stage refuses unless the recipe is `pr-stack` (`stack_seed_validation.rs:38-46`, `run.rs:1748`).

What to delete in PR 1:
- the web picker (`CreateSessionToolFields.tsx:86-108`, `useCreateSessionStack.ts`, `stackBaseSessionCandidates` and `sessionsWithBranch` in `stackParents.ts`)
- the request field (`createSessionRequest.ts`)
- the proto field `pr_stack_base_session_id = 31` (and regenerate both TS copies)
- `tool-engine` start_session_args and schema, plus `start_options` `pr_stack_base_candidates` and `stack_base_session_candidates`
- the daemon gate (`svc_start_session_core.rs:272-288`, `start_request_checks.rs:456-472`, `stack_seed_validation.rs`)
- spawn plumbing (`tool_session_spawn.rs`, `tool_spawn_plan.rs`, `spawner.rs` `SpawnOptions` field plus the `pr_stack_spawn_args` third tuple, `spawn_worker.rs`, `supervisor_spawn.rs`, and the `None` sites in resume, project_home_start_spec, telegram and config)
- the CLI (`run.rs` arg, mapping, call at :1381, both functions)
- the writer `tddy-pr-stack/stack_ops/seed.rs`

Tests that cover it:
- `tddy-coder/tests/pr_stack_seed_start_acceptance.rs`
- `session-lifecycle/tests/pr_stack_base_session_spawn_acceptance.rs`
- `recipes/tests/pr_stack_base_session_seed_acceptance.rs`
- `session-create-web/cypress/component/CreateSessionPrStackBaseSessionAcceptance.cy.tsx`, `CreateSessionRequestBuilder.cy.ts:143-168`, `createSessionRequestParity.cy.ts:54-58`, `fixtures/start-session-tool-fields.json:25`
- `tool-engine/tests/start_session_schema.rs:229,275`

**A6. `PrStackScreen` and its dropdown entry.**
- It is reached only through `session-web/workflowViews.tsx:106` (recipe `"pr-stack"`) and `attachClaim.ts:50`. The dropdown entry is `createSessionRecipes.ts` `"pr-stack"`.
- Modules used only by PrStackScreen: `PrStackStartSessionDialog`, `usePrStackStartSession`, `usePrStackPlan`, `usePrStackBranchMutations` and `stackDocAttachments.ts`.
- `PlannedPrPanel` and `PrStackChat` are used by PrStackScreen and are also exported. `PlannedPrPanel` has a storybook story, and `PrStackChat` is mounted directly by `session-web PrStackChatFeedbackAcceptance`, `PrStackChatSilentFailureAcceptance` and `tddy-web PrStackChatNoRoomAcceptance`.
- **Shared with `ProjectStackView` (keep):** `PlannedPrList`, `PlannedPrRow*`, `AddPlannedPrForm`, `DirtyWorktreeDialog`, `startBlockers`, `hydrateStackNodes`, `stackChildSessions`, `useQueryBranch`, `branchQueries`, `baseSyncStatus`, `deriveStackBaseBranch`, `baseBranchChoice`, `stackPlan`. `ProjectStackView` is the project-record view mounted by `SessionsDrawerScreen` and `SessionsDrawerDetailPane`.
- **Tests that drive PrStackScreen through a `recipe:"pr-stack"` session** (39 specs under `session-web/cypress/component/`): all 33 `PrStack*.cy.tsx`, plus `PlannedPrRowInternalStatusAcceptance`, `WorkflowChatViewRoutingAcceptance` (fixture recipe at :51), `InactiveSessionActivitiesAcceptance:89` and `WorktreeCodePaneAcceptance:71`. Others: prstack-web `PrStackScreenDirectMountAcceptance`, `PrStackChat*`, `PlannedPrListStartSessionDirectMountAcceptance`; tddy-web `PrStackPresenterRoomAcceptance`, `PrStackChatTokenFailureAcceptance`, `PrStackChatNoRoomAcceptance`, the e2e `cypress/e2e/pr-stack-chat-response.cy.ts` plus `cypress.config.ts:974` task `startDaemonForPrStack` and the `package.json:22` script `cypress:e2e:prstack`; unit test `attachClaim.test.ts:50,74`.
- Only `SessionsDrawerProjectStacksAcceptance.cy.tsx` uses the `ProjectStackView` path. Much of the planned-PR row coverage (reorder, repoint, sync, branch query, start dialog) is exercised **only** through PrStackScreen, so deleting those specs removes that coverage unless they are re-pointed at `ProjectStackView`.

**A7. Other recipe-specific code and tests.**
- `recipes/tests`: tests that exercise the recipe or hooks are `pr_stack_artifact_paths_acceptance`, `pr_stack_docs_*`, `pr_stack_free_prompting_acceptance`, `pr_stack_seeding_acceptance`, `pr_stack_store_resume_acceptance`, `plan_pr_stack_acceptance`, `orchestrate_pr_stack_{bridge,github_on_demand,loop,resume}_acceptance`, `pr_stack_display_order_acceptance` (uses StackPlanOutput) and `pr_stack_crate_shape.rs` (asserts `pub struct PrStackRecipe` exists).
- Tests that exercise data-model operations through the recipe's re-export paths: `pr_stack_branch_link`, `pr_stack_pr_adoption`, `pr_stack_pr_sync`, `pr_stack_pull_base`, `pr_stack_realign_failure`, `pr_stack_repoint*`, `pr_stack_set_parents`, `pr_stack_internal_status*`, `pr_stack_pr_insight`, `orchestrate_pr_stack_merge_repoint`, `pr_management_actions`, `pr_status_view`, `github_qualified_head`, `common/mod.rs`. These need their imports repointed, not deletion.
- `StackChildSpawnHandler` (`agent-launch/stack_child_spawn.rs`, `child_spawn_handler.rs`) is bound only for `pr-stack` (`svc_start_claude_cli_session.rs:44-66`). The todo `2026-11-14-retire-stack-child-spawn-handler.md` says it is unreachable. The `ChildSpawnHandler` trait itself is shared with `conversation_spawn.rs` (grill-me) and toolcall, so keep the trait.

**A8. What breaks outside the recipe if the three recipe modules are deleted naively.**
1. `daemon-rpc/pr_stack/ports.rs` and `project_stack_queries.rs` call stack_ops through `tddy_workflow_recipes::pr_stack::…`. Repoint them to `tddy_pr_stack::stack_ops::…`.
2. `daemon-rpc/pr_stack/{pr_status,branch_legs,ports}.rs`, `exec_tool/stack_tools/ops.rs`, `exec_tool/stack_git_tools/git_work{,/node_ops}.rs` and `tddy-tools/src/github_credential.rs` (cfg(test)) use `orchestrate_pr_stack::{github::*, worktree_is_clean, derive_internal_status, reconcile_internal_status, StackOpJournal, MergePhase, RealGithubPrApi}`.
   - `github` is just `tddy_github::pr_api`.
   - `worktree_is_clean` is `tddy_pr_stack::git_ops`.
   - **`internal_status.rs` and `transient.rs` (StackOpJournal, MergePhase, recover_in_flight_stack_op) and `bridge.rs` (`execute_stack_merge`, `execute_stack_repoint`) live only in `tddy-workflow-recipes` and are used by the daemon's live stack git tools.** They must move to `tddy-pr-stack` before the orchestrate module can go.
3. `tddy-session-files` imports the `NODE_*` consts and `PR_STACK_PLAN_MD_BASENAME`. The `NODE_*` consts are already in `tddy_pr_stack::docs`. `PR_STACK_PLAN_MD_BASENAME` lives in `plan_pr_stack/prompt.rs`.
4. `tddy_workflow_recipes::PR_STACK_SERVICE` is a re-export of `tddy_pr_stack::PR_STACK_SERVICE`, used in 4 daemon-rpc sites.
5. `tddy-tools/tests/pr_stack_tool_dispatch_acceptance.rs` is built on `PR_STACK_TOOL_NAMES`.
6. `tddy-coder` uses `STATE_STACK_*` and the seed functions (this goes with A5).
7. `session-worktree parent_is_pr_stack_orchestrator` survives through `stack_id`.
8. `session_list_enrichment.rs` tests and fixtures carry `recipe: pr-stack` strings. There is no production recipe gate there: `stack_plan_json_for_changeset` keys on `stack_id`.
9. `tddy-pr-stack stack_migration.rs` and `stack_ops/mod.rs` fixtures write `recipe: Some("pr-stack")` and are harmless. Its comments say the recipe "stays" in recipes.
10. Changesets already on disk with `recipe: pr-stack|plan-pr-stack|orchestrate-pr-stack` will fail resolution (`unknown_workflow_recipe_error`) on resume or listing once removed. The migration or deprecation behavior needs a decision.

**A9. Docs describing the recipe.**
- `docs/ft/coder/pr-stacking.md` (468 lines): summary, § pr-stack recipe :166-221, Legacy aliases :216, Seeding :103-142, CLI flags :146-153, Progress contract :309-323, Web UI :375-460.
- `docs/ft/coder/pr-stack-docs.md` (329), `pr-stack-live-status.md` (918), `workflow-recipes.md:16,21`, `exploration-artifact.md` (12 hits), `docs/ft/web/session-drawer.md` (77 hits, per-workflow views), `docs/ft/web/prstack-package.md`.
- Package docs: `tddy-pr-stack/docs/architecture.md` (20), `tddy-daemon/docs/pr-stack-service.md` (9), `tddy-prstack-web/docs/README.md` (7), `tddy-daemon-rpc/docs/architecture.md` (7), `tddy-workflow-recipes/README.md` (5), and `packages/tddy-daemon/docs/connection-service.md` § Stack-seed base session (linked from pr-stacking.md).
- Changelogs and changesets are historical and are normally left alone.
- `.agents/commands/*pr-stack*` and `.agents/skills/pr-stack` are the **developer's GitHub-PR-stack workflow**, not the product recipe. Exclude them from both PRs.

#### B. Rename surface

**B1. `pr_stack.proto`**
- `package pr_stack`, `service PrStackService`.
- 11 RPCs: `AddPlannedPr`, `GetPrStatus`, `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch`, `QueryBranch`, `ResolveStackBase`, `LinkStackNode`, `ListProjectStacks`, `GetProjectStack`, `CreateProjectStack`.
- 31 messages: `CreateProjectStackRequest/Response`, `StackAddress`, `AddPlannedPrRequest/Response`, `BranchBaseSync`, `BranchRemote`, `BranchResolution`, `BranchWorktree`, `GetPrStatusRequest/Response`, `LinkStackNodeRequest/Response`, `PrStatusView`, `PullBaseIntoBranchRequest/Response`, `QueryBranchRequest/Response`, `ReorderPlannedPrRequest/Response`, `RepointPlannedPrRequest/Response`, `ResolveStackBaseRequest/Response`, `ListProjectStacksRequest`, `StackNodeCounts`, `ProjectStackSummary`, `UnreachableStackHost`, `ListProjectStacksResponse`, `GetProjectStackRequest/Response`.
- Related wire fields elsewhere:
  - `session.proto`: `SessionEntry.stack_plan_json = 23`, `stack_id = 38`, `orchestrator_session_id`; `StartSessionRequest.stack_parent = 15`, `pr_stack_base_session_id = 31`, `stack_parent_daemon_instance_id = 36`, `stack_node_id = 37`.
  - `types.proto` `BranchSession` comment at :81.
- **Generated consumers:** Rust (32 files, listed in §3) and TS (`gen/pr_stack_pb.ts` in two packages, plus 49 importers). Codegen keys `"pr_stack"` appear in `build.rs:65,86` and `build/unbundled_families.rs:181,206-207`.
- Persisted or transport identifiers: `PR_STACK_SERVICE = "pr_stack.PrStackService"` (`tddy-pr-stack/rpc.rs:25`), its assert in `workflow-recipes/lib.rs:94`, `daemon-rpc/tests/support/mod.rs:56`, and the closed list in `unbundle_service_split.rs:548-559, 627-634`. That list is missing `CreateProjectStack`.

**B2. `pr_*` tools (17).**
- Plan and read (11, `tool-engine/stack_tools.rs`): `pr_stack_list`, `pr_stack_create`, `pr_stack_status`, `pr_read`, `pr_search`, `pr_comments`, `pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt`.
- Git (6, `stack_git_tools.rs`): `pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts`, `pr_spawn_child`.
- The argument key `stack_id` is `STACK_ID`.
- Closed registries to update together: `STACK_TOOL_NAMES[11]`, `STACK_GIT_TOOL_NAMES[6]`, `read_only.rs:152-168`, `tddy-tools/exec_tool_admission.rs:211-227`, the recipe `PR_STACK_TOOL_NAMES[15]` (deleted in PR 1), `stack_tools_catalogue.rs:21-33`, `svc_list_exec_tools_wiring.rs:250`, `caller_records.rs:405`, `mcp_tool_advertisement_audit.rs` comments and counts, and the agent prompt `project_home_prompt.md:72-75`. Tool descriptions in `stack_tools/defs.rs` cross-reference each other by name ("use pr_add_planned", "pr_stack_list shows it").

**B3. Rust types.**

| Crate | Types |
|---|---|
| tddy-changeset | `Stack`, `StackNode`, `StackRecord`, `StackStoreError`, `ProjectStackStore` (`project_stack_store` module, on-disk `projects/<id>/stacks/<stack_id>.yaml`), `GithubPrStatus`, `PrInternalStatus`, `Changeset.{stack, stack_id, orchestrator_session_id}`, `stack_fixtures::AStackHost` |
| tddy-pr-stack (crate name itself) | `PrStackHandler`, `PrStackServiceImpl`, `PR_STACK_SERVICE`, `StackHome`, `StoredStack`, `AddPlannedPrInput`, `UpdatePlannedPrInput`, `StackSeedBaseRefusal`, `StackDocsOutput`, `PrLiveStatus`; functions `add_planned_pr_node`, `delete_planned_pr_node`, `update_planned_pr_node`, `repoint_planned_pr_node`, `move_planned_pr_node`, `adopt_pr_as_stack_node`, `migrate_session_stacks` |
| tddy-daemon-rpc | `PrStackRpcHandler`, `StackTarget`, `StackGithub`, `StackGithubClients`, `RealStackGithubClients`; module `pr_stack/` (ports, project_stacks, project_stack_queries, branch_legs, pr_status, guards) |
| tddy-tool-engine | `StackCaller`, `StackCallerRole`, `ParentStack`, `StackCallerFacts`, `StackScope`, `StackOp`, `StackToolPort`, `StackGitOp`, `StackGitToolPort`, `StackCallerUnreadable` |
| tddy-agent-launch | `StackChildSpawnHandler`, `StackBaseLookup`, `StackNodeLink`, `StackParentHost`, `SpawnStackParent`; `svc_pr_status_for_caller.rs` |
| tddy-session-worktree | `StackParentRoute`, `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn` |
| tddy-session-lifecycle | `pr_stack_rpc.rs`, `rpc_families::pr_stack_handler` |
| tddy-workflow-recipes (deleted or moved in PR 1) | `PlannedPr`, `StackPlanOutput`, `StackOpJournal` |

**B4. Web components in `tddy-prstack-web`.**
- Components and modules: `PrStackScreen`, `PrStackChat`, `PrStackStartSessionDialog`, `PlannedPrList`, `PlannedPrPanel`, `PlannedPrRow`, `PlannedPrRowBadges`, `PlannedPrRowDetails`, `AddPlannedPrForm`, `DirtyWorktreeDialog`, `ProjectStackView` (`ProjectStackAddress`), `WorkflowChatScreen`, `usePrStack{Plan,StartSession,BranchMutations}`, `stackPlan`, `StackNode` TS type, `stackChildSessions`, `hydrateStackNodes`, `orderStackNodes`.
- Session-web components: `ProjectStacksRow.tsx`, the `prStackClient` prop threading (`sessionMainPaneProps.ts:108`, `SessionDrawer.tsx:91`, `useSessionsDrawerClients.ts:7,50`, `useSessionsDrawerOwningHost.ts`, `sessionPaneClients.ts`, `sessionManager.ts`).
- **The package is named `tddy-prstack-web`**, and the name appears in `package.json` dependencies (`tddy-web/package.json:47`).
- 59 `data-testid` attributes in prstack-web, all with the prefix `pr-stack-` except three:
  - `AddPlannedPrForm`: `pr-stack-add-planned-pr-{form,title-input,description-input,branch-suggestion-input,cancel-btn,submit-btn,start-btn,error,ancestor-${id}}`
  - `DirtyWorktreeDialog`: `pr-stack-dirty-worktree-{dialog,paths,commit-message-input,commit-btn,cancel-btn}`
  - `PlannedPrList`: `pr-stack-planned-pr-list`
  - `PlannedPrPanel`: `pr-stack-planned-pr-panel`, `-close`
  - `PlannedPrRow`: `pr-stack-{planned-pr-row,row-toggle,session,start-session,start-session-blocked-icon,start-warning,status-chip,repoint,repoint-error,reorder-error,sync-error}-${id}`
  - `Badges`: `pr-stack-{base-behind,base-conflicts,base-in-sync,base-sync-unavailable,in-progress,internal-status-badge,pr-link,pr-state,pr-unavailable}-${id}`
  - `Details`: `pr-stack-{base-branch,base-conflict-paths,branch,child-recipe,child-session,child-state,move-up,move-down,node-id,parents,planned-branch,row-details,sync-merge,sync-rebase,worktree}-${id}`
  - `PrStackScreen`: `pr-stack-screen`, `pr-stack-add-planned-pr-btn`, `pr-stack-planned-pr-panel-toggle`
  - `ProjectStackView`: `project-stack-view`, `project-stack-new-stack-btn`, `pr-stack-add-planned-pr-btn`
  - `WorkflowChatScreen`: `workflow-chat-screen`
- The ids are duplicated as string constants in `cypress/support/testIds.ts` of prstack-web (56), session-web (96) and tddy-web (91), plus `pages/prStackScreenPage.ts` in all three and `projectStackViewPage.ts` and `projectStacksPage.ts`.
- Other testids: `sessions-drawer-project-stacks-${projectId}`, `sessions-drawer-project-stack-${stackId}`, `sessions-drawer-stack-…`, `create-session-stack-parent-select`, `create-session-pr-stack-base-session-select` (deleted in PR 1).

#### C. Backlog entries touching these areas

Only one carries a `**Claimed by:**` line (it reads "— unclaimed"). All the others carry none, so they are all unclaimed.

**`docs/dev/todo/`, directly relevant:**

| Entry | Gist |
|---|---|
| `2026-07-26-pr-stack-status-polling-and-stack-hygiene.md` (PR-Stack — status polling and stack hygiene) | Manual live verification never done, plus polling and hygiene follow-ups |
| `2026-07-30-pr-stack-full-control-follow-ups.md` | Split `pr_stack/mod.rs` and `orchestrate_pr_stack/github.rs`, a `PrSearchState` enum, deduplicate the appenders, a testable transport for `RealGithubPrApi` |
| `2026-08-13-pr-stack-a-managed-claude-cli-cursor-cli-session-may-record-a-goal-id.md` | `PrStackRecipe::start_goal()` "analyze-stack" is persisted as a state by CLI spawns. Dies with the recipe |
| `2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md` | `session_repo_is_in_project` refuses worktrees outside `.worktrees/`. Dies with the seed flag |
| `2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions.md` | Multi-session seeding was deferred. Dies with the seed flag |
| `2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md` | Hoist the trim helper; move `validate_stack_seed_base_session` / `require_pr_stack_orchestrator` into a `pr_stack.rs` |
| `2026-08-13-tddy-daemon-generalize-pr-stack-spawn-args-to-all-optional-spawn-flags.md` | Rename `pr_stack_spawn_args` to `optional_flag_args`. Touched by both PRs |
| `2026-08-13-tddy-web-a-failed-listsessions-is-indistinguishable-from-an-empty-resu.md` | The form swallows a ListSessions failure that feeds the "Base the stack on" and parent pickers |
| `2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md` | The stack-progress hook was never implemented. Resolve by deleting the doc |
| `2026-08-29-grillmerecipe-ships-blank-context-doc-descriptions.md` | Cites `PrStackRecipe`'s `artifact_doc_descriptions` (`pr_stack/mod.rs:342-362`) as the model. The reference goes stale |
| `2026-08-30-pr-stack-querybranch-s-local-only-legs.md` | `QueryBranch` worktree and remote legs are local-only. Rename surface |
| `2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md` (Known failing test) | `stack_child_spawn_tests.rs:395` flakes. Related to the `StackChildSpawnHandler` path |
| `2026-10-25-new-session-form-offers-8-of-10-recipes.md` | The form's `WORKFLOW_RECIPES` (8) differs from the daemon's 10. Changes when the 3 pr-stack names go |
| `2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md` | The StartSession recipe is a free string in MCP. The recipe enum shrinks |
| `2026-10-25-orchestrated-tool-session-start-refused.md` | An orchestrated `tool` start is refused (it needs a `tddy-coder` arg like `--stack-parent`) |
| `2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md` | `pr_stack_status` and `pr_read` are not listed cross-host. Rename surface |
| `2026-11-14-retire-stack-child-spawn-handler.md` | `StackChildSpawnHandler` (bound for `pr-stack` only) is unreachable. Retire it. Overlaps PR 1 |
| `2026-07-21-tddy-workflow-recipes-tddy-discovery.md` | Exploration-artifact enhancements (pr-stack writes `exploration.md`). Tangential |

**`docs/dev/todo/`, incidental (they cite pr-stack files or paths, mostly `daemon-rpc/src/pr_stack/ports.rs` size):**
- `2026-08-01-cargo-test-workspace-has-7-pre-existing-failures…` (measured at the pr-stack-base-session wrap)
- `2026-08-13-wrapping-a-changeset-leaves-dangling-1-wip-pointers…` (source: pr-stack-base-session)
- `2026-09-10-tddy-spawn-modules-are-over-budget…` (`pr_stack_spawn_args`)
- `2026-09-24-lifecycle-functions-still-over-150-lines.md` (CLI spawns stop at the PR-stack parent)
- `2026-10-03-start-phases…` (pr-stack orchestrator children skip start phases)
- `2026-10-08-keyring-github-identity-oversized-files.md`, `2026-10-15-local-gh-oversized-files-deferred.md`, `2026-11-20-session-liveness-oversized-files-after-the-stack.md` (`pr_stack/ports.rs` growth)
- `2026-11-07-session-web-specs-do-not-typecheck.md` (PrStack* specs)
- `2026-11-08-cypress-attr-helpers-freeze-the-first-value.md` (`prStackScreenPage.ts`)
- `2026-11-14-restructure-engine-no-extract-for-guarded-ranges.md` and `2026-11-14-restructure-engine-path-module-blocks-moves.md` (both cite `pr_stack/ports.rs`)
- `2026-09-06-from-the-…-optional-livekit-stack-evaluation.md` is about the developer PR-stack process, not the product

**`packages/*/docs/code-issues/`:**

| Entry | Gist |
|---|---|
| `tddy-workflow-recipes/…/squatting-pr-stack-data-model.md` | The only one with **Claimed by: "— unclaimed. #496 delivered the extraction…"**. Status: partially fixed. Consumers still reach `tddy-pr-stack` through `tddy-workflow-recipes`. Directly addressed by PR 1's repointing |
| `tddy-workflow-recipes/…/complexity-bridge-execute-stack-repoint.md` | `orchestrate_pr_stack/bridge.rs:110`, 121 lines. Moves with the helper |
| `tddy-workflow-recipes/…/misplaced-tests-github-rest-client.md` | `github_qualified_head_acceptance`, `pr_status_view_acceptance` and `github_pr_acceptance` test `tddy_github` through `orchestrate_pr_stack::github` |
| `tddy-daemon-rpc/…/oversized-file-pr-stack-ports.md` | `pr_stack/ports.rs` is 775 → 880 lines; accepted as standing debt |
| `tddy-pr-stack/…/complexity-pull-base-pull-base-into-node-branch.md`, `complexity-repoint-repoint-planned-pr-node.md` | Rename surface (`planned_pr`) |
| `tddy-github/…/oversized-file-pr-api.md` | Was `orchestrate_pr_stack/github.rs` |
| `tddy-tools/…/oversized-file-server.md` | Mentions pr tools |
| `tddy-coder/…/oversized-file-run.md` | `run.rs` shrinks when the seed flag goes |
| `tddy-tool-engine/…/complexity-start-session-args-schema-schema-offering.md` | Includes the `pr_stack_base_session_id` offer |
| `tddy-agent-launch/…/complexity-remote-agent-launch-launch-on-target.md` | Only incidental |

None of these code-issue entries has a "Claimed by" line except the squatting entry.

#### Other facts

- `docs/dev/1-WIP/2026-11-22-workqueue-whole-work-initial-discovery.md` already exists. It is an 11-line placeholder ("Passes: 0", "Combined Conclusions (pending)").
- The project-level stack (the `ProjectStackStore` record addressed by `stack_id`, role-gated tools, `ProjectStackView`, `ListProjectStacks` / `GetProjectStack` / `CreateProjectStack`) is already independent of the recipe (`#orchestrators` 7–13). The recipe is a legacy way into the same store.
- `session_list_enrichment::stack_plan_json_for_changeset` keys on `Changeset.stack_id`, not on the recipe.

## Exploration 2 — GitHub access, the session inspector, role checks

I couldn't run every request as asked. On `origin/investigate/github-api-rate-limit` the only change against master is `AGENTS.md`, so it has no other notes to read. On item A: the daemon has exactly one conditional (ETag/304) GitHub poller today, the trunk feed. Every other GitHub read is an unconditional curl with no cache. On item B: a Worktree inspector tab already exists and no PR tab exists. On item D: no relevant backlog or code-issue entry is claimed by anyone.

### 1. Sequence

1. `git status`, `ls packages`, `find packages/tddy-github -type f`: worktree state, package list, files in tddy-github.
2. `git fetch` of the investigate branch, then `git log origin/investigate/github-api-rate-limit --oneline -20` and `git diff master...origin/investigate/github-api-rate-limit --stat`, as you asked.
3. `git diff master...origin/...` (full) and `git show 4ebc40364`: the only changed path was `AGENTS.md`, so I read both versions of the note.
4. `wc -l` on tddy-github sources, then read `lib.rs`: module inventory.
5. Read `trunk_feed.rs`, `docs/trunk-feed.md` and `github_rest_common.rs`: the ETag poller and the HTTP transport.
6. Grepped `pr_api.rs`, `github_pr.rs` and `real.rs` for their public items; read `README.md`, `docs/rest-token.md` and `Cargo.toml`.
7. Read `pr_api.rs` lines 1–420 and 830–1090: traits, data types, token source, REST implementations.
8. Grep across `packages` (Rust/TS) for `api.github.com|get_pr_by_head|GithubPrInsightApi|list_check_runs|RealGithubPrApi|TrunkPoller|RestTrunkFeed|GhCliTrunkFeed`: every caller of the GitHub client.
9. Grep for `Command::new("gh")|"pr","view"|statusCheckRollup|graphql|"api","-i"` and for `"gh"` in non-test Rust: every place that shells out to `gh`.
10. Read `tddy-session-activity/src/trunk_watch.rs`, the package changeset `2026-11-21-trunk-watch.md`, the root changeset `docs/dev/changesets/2026-11-21-trunk-watch.md` and `svc_trunk_watch_wiring.rs`.
11. Role-check greps (`subscribe_orchestrator_to_trunk|orchestrator_running|is_orchestrator_role|ORCHESTRATOR_ROLE|PROJECT_HOME_ROLE|session_role`), then `sed` context around every hit.
12. Read `tddy-changeset/src/session_metadata.rs` (the `SessionMetadata` struct and the role constants).
13. TS grep for `sessionRole|"orchestrator"|"project-home"|isOrchestrator`: role checks on the web side.
14. Listed `tddy-session-web/src`; read `InspectorTabs.tsx`, `SessionInspectorDrawer.tsx`, `inspectorState.ts` and `useSessionsDrawerInspector.ts`.
15. Grep for `InspectorTabName` and `SessionWorktreeTab`; read the tab list in `tddy-client-web/src/appRoutes.ts` (lines 230–290).
16. Read `worktree.proto`, `SessionWorktreeTab.tsx` and `useSessionWorktreeStats.ts`.
17. Grep for RPCs in `session.proto`, `pr_stack.proto`, `worktree_activity.proto`, `remote_git.proto` and `actions.proto`; read `SessionEntry` and `GetWorktreeSnapshot`.
18. Grep for `snapshot_worktree|left-right|ahead|behind|unpushed|unpulled`; read `worktree_measurement.rs`, `base_freshness.rs` and `base_sync.rs` (header and lines 170–220).
19. Read `pr_stack/pr_status.rs`, `pr_stack/ports.rs`, `pr_stack/branch_legs.rs` and `tddy-prstack-web/src/useQueryBranch.ts`.
20. Grep for web polling (`setInterval|POLL_INTERVAL|queryBranch|getPrStatus`); found the two `useQueryBranch(` callers.
21. Read `tddy-pr-stack/src/pr_insight.rs`; grepped `assess.rs`, `adopt.rs`, daemon `stack_tools.rs`, `tddy-tools`, `merge_pr/github.rs` and `orchestrate_pr_stack/actions.rs`.
22. Read the header of daemon-rpc `exec_tool/stack_tools.rs`, plus `stack_tools/github.rs` and `stack_tools/ops.rs` (the `run` and `status` functions).
23. Read `tddy-tools/src/github_credential.rs`, `tddy-host-service/src/github_cli.rs` (lines 140–240), `tddy-daemon-auth/src/github_pr_credentials.rs` and `tddy-agent-launch/src/session_acting_identity.rs` (`project_github_token`, `local_gh_github_token`).
24. Backlog scan: `grep -rli` over `docs/dev/todo` and `packages/*/docs/code-issues`, narrowed by regex; printed titles and searched for "Claimed by"; read six todo entries and five code issues.
25. Found inspector Cypress specs, stories and feature docs; read the head of `docs/ft/web/session-worktree-inspector.md`, the outlines of the URL-state, inspector and worktree-tab specs, the page object, and the test ids.
26. Read the `Changeset` model (`branch`, `github_pr_status`, `GithubPrStatus`) and grepped who writes `github_pr_status`.
27. Read the session-room poll interval grep, `SessionNotificationSource`, `WatchKind`, `WATCH_KINDS`, the self-watch admission check, and the daemon runtime wiring in `mount.rs`.
28. Read `docs/ft/coder/pr-stack-live-status.md` (polling lines) and `docs/ft/daemon/project-concept.md` § Watching the trunk.

### 2. Inspected files (with the excerpts that matter)

All paths below are under `/Users/mantasi/Code/tddy-coder/.worktrees/plan-workqueue-planner/`.

#### A. GitHub access

**`packages/tddy-github/src/lib.rs`**
- Modules: `auth_service`, `github_pr`, `github_rest_common`, `pr_api`, `provider`, `real`, `session_token_v2`, `stub`, `trunk_feed`.

**`packages/tddy-github/src/trunk_feed.rs`** (404 lines): the only conditional GitHub reader.
- Constants: `DEFAULT_POLL_INTERVAL_MS = 60_000` (:25), `ERROR_BACKOFF_MS = 60_000` (:27), `SWEEP_EVERY_MS = 15 * 60_000` (:29).
- `Conditional<T>` (:52): `NotModified { poll_interval_ms }` or `Modified { value, etag, poll_interval_ms }`. This is a reusable shape for a per-row ETag cache.
- `trait TrunkFeed { async fn newest_commits(repo, branch, etag) }` (:77).
- `TokenSource = Arc<dyn Fn() -> Result<String, TrunkReadError>>` (:88). It is called on every read.
- `RestTrunkFeed` (:91–150):
  - builds a fresh `reqwest::Client::new()` per call;
  - sends `If-None-Match`, `User-Agent: tddy-trunk-watch` and `X-GitHub-Api-Version: 2022-11-28`;
  - maps `304` to `NotModified`, `200` to `Modified`, anything else to `TrunkReadError`;
  - reads `x-poll-interval` into `poll_interval_ms`;
  - has no rate-limit header handling (`X-RateLimit-*` and `Retry-After` are never read).
- `trait GhApi { api_with_headers(path, if_none_match) }` (:162) and `GhCliTrunkFeed` (:171–223). For the `gh` path, the HTTP status line decides the outcome, never the exit code (:210).
- `TrunkPoller` (:242–333): a step function with no loop or clock of its own. `poll(now) -> PollOutcome { changes, next_poll_at_unix_ms }`.
  - The first answer is a baseline and reports nothing.
  - New commits are reported as `Landed` (oldest first); a missing previous tip is reported as `Rewritten`.
  - Every 15 minutes the read is unconditional (`sweep`).
- Path: `repos/{o}/{n}/commits?sha={branch}&per_page=100` (:335).

**`packages/tddy-github/src/github_rest_common.rs`**: the curl transport behind every other GitHub call.
- Shells out to `curl -sS -L -o <tmpfile> -w %{http_code}` (:58, :170).
- Headers: `Authorization: Bearer`, `Accept: application/vnd.github+json`, `User-Agent: tddy-coder-workflow-recipes` and the API version header.
- `require_token` refuses a blank token (:23).
- Any non-2xx is `Err(...)` with the body. There is no ETag, no 304 handling, no response-header capture and no rate-limit handling.
- Functions: `curl_github_{get,post,patch,put}_json_with_token(api_base, repo, path, …)` and `curl_github_get_json_absolute_path`.

**`packages/tddy-github/src/pr_api.rs`** (1428 lines)
- `trait GithubPrApi` (:78): `get_open_pr`, `get_pr_by_head -> PrLookupOutcome`, `merge_pr`, `patch_pr_base`, `create_pr`, `disable_auto_merge`, `close_pr`.
- `PrLookupOutcome { Found(PrView), NotFound, Unavailable(String) }` (:36). The comment says "could not be performed (no credential, rate limit, transport error) must never read as 'this branch has no PR'".
- `trait GithubPrInsightApi` (:222):
  - `get_pr` → `GET pulls/{n}`;
  - `list_pr_files` → `/files`;
  - `list_check_runs(head_sha)` → `GET commits/{sha}/check-runs`;
  - `list_reviews`, `list_review_comments` (`pulls/{n}/comments`), `list_issue_comments` (`issues/{n}/comments`);
  - `search_prs` (`/search/issues`), `patch_pr_title_body`.
- Data types:
  - `PrDetail` (:114): number, url, title, body, state, base/head branch, head_sha, mergeable, mergeable_state, additions/deletions/changed_files.
  - `CheckRun { name, conclusion }` (:141). There is no `status`, `details_url` or `id`.
  - `PrReviewComment { id, in_reply_to_id, author, body, path, line, diff_hunk, created_at }` (:165). Resolved state is deliberately absent because it is GraphQL-only.
  - `PrIssueComment { author, body, created_at }` (:179). There is no `id`.
- Token sources (:265): `Explicit`, `Absent`, `Asked { ask, answered: OnceLock }`. The rule "no environment variant, and there must never be one" is stated there.
- `get_pr_by_head` (:887) runs `GET pulls?state=all&head=owner:branch` and takes the first entry.
- `PER_PAGE = "100"`, with pagination deliberately off (:1032–1037).

**`packages/tddy-github/README.md` and `docs/rest-token.md`**
- REST calls carry a token handed to them; nothing reads `GITHUB_TOKEN`/`GH_TOKEN`.
- `RealGithubPrApi::{with_token, asking, asking_the_session_host, without_credential, with_api_base}`.

**`packages/tddy-github/docs/trunk-feed.md`**: same rules as the code; "Only `github.com` remotes are read".

**`packages/tddy-session-activity/src/trunk_watch.rs`** (241 lines)
- Header (:3): "Orchestrator-role sessions subscribe when they start or resume … One `TrunkPoller` serves every subscriber of a project".
- `TrunkSubscription { project_id, os_user, session_id, github_subject }` (:31).
- `trait TrunkSource { poller_for(sub) -> (TrunkPoller, trunk_ref) }` (:43) and `trait SubscriberLiveness { is_live }` (:51).
- `TrunkWatcher` (:96) holds `projects: Mutex<HashMap<project_id, ProjectPoller>>`.
  - `subscribe` builds a project's poller on its first subscriber (:123).
  - `unsubscribe` drops a poller with its last subscriber (:145).
  - `step(now)` sweeps out dead subscribers, takes the due pollers out of the lock, polls, and publishes each change once per subscriber as a `SessionNotification { source: TrunkCommit, kind: Activity, label: "orchestrator" }` (:163–184, :221).
- Sharing is per project; fan-out is per subscriber; there is no event-kind filter (there is only one kind).

**`packages/tddy-session-lifecycle/src/connection_service/svc_trunk_watch_wiring.rs`** (295 lines)
- Driver step wait is clamped to 1–5 s: `LONGEST_STEP_WAIT = 5s` (:32), `SHORTEST_STEP_WAIT = 1s` (:34). `START_GRACE = 60s` (:36).
- `HostGh` implements `GhApi` via `GithubCli::api_with_headers` on `spawn_blocking` (:80–107).
- `HostTrunkSource::feed_for` (:135):
  - `row.local_gh` → `GhCliTrunkFeed`;
  - otherwise → `RestTrunkFeed` whose token closure re-resolves `project_account_token(host, sub, subject)` from the subscribing GitHub user's vault (:157).
- `poller_for` (:164) takes the repo from `owner_repo_from_remote_url(row.git_url)` (github.com only) and the branch from `effective_integration_base_ref_for_project`.
- `subscribe_orchestrator_to_trunk` (:239) and `spawn_trunk_watch` (:270; refreshes the `live_sessions` snapshot, then `watcher.step(now)`).
- Installed in `packages/tddy-daemon/src/runtime/mount.rs:96` (`.with_trunk_watch(...)`) and started at `:107` (`spawn_trunk_watch()`).

**Trunk-watch changesets** (`docs/dev/changesets/2026-11-21-trunk-watch.md`, `packages/tddy-github/docs/changesets/2026-11-21-trunk-watch.md`)
- "The commits list, not the events feed": the PRD's events-feed plan was changed at the contract.
- `SessionNotificationSource::TrunkCommit` and `WatchKind::TrunkCommit`. `CreateSessionWatch` admits a self-watch of `trunk_commit` alone.
- Known limits: github.com only; more than 100 commits between polls is reported as a rewrite.

**`packages/tddy-host-service/src/github_cli.rs`**
- `auth_token(os_user, hostname)` runs `gh auth token --hostname` (:143).
- `api_with_headers` runs `gh api -i --hostname H [-H If-None-Match: …] <path>` and returns `(stdout, exit_code)` (:163–186). Each run is bounded by `PROBE_TIMEOUT`.

**`packages/tddy-daemon-rpc/src/pr_stack/pr_status.rs`**
- `pr_status_for_caller(github_login, repo_root, branch)` (:17) uses the login-keyed `retained_github_token(credential_vaults, github_login)` (:56), not the project account, then calls `RealGithubPrApi::with_token(owner_repo, token).get_pr_by_head(branch)` on `spawn_blocking` (:68–75).
- Uncached; one `GET /pulls?state=all&head=` per call.
- `owner_repo_from_repo_root` runs `git remote get-url origin` (:156).

**`packages/tddy-daemon-rpc/src/pr_stack/ports.rs`** (880 lines)
- `get_pr_status` (:115) and `query_branch` (:148). QueryBranch's legs:
  - session (`find_session_owning_branch`);
  - worktree (`worktree_leg`);
  - remote (`remote_branch_ref_sha`);
  - base_sync (cached `merge-tree`);
  - pr (`pr_status_for_caller`, :277).
- Comment at :235: "this handler is polled once per rendered row every few seconds".
- `pull_base_into_branch` also calls `pr_status_for_caller` (:831).
- `repoint_planned_pr` resolves a project-account or `local_gh` token (:560–587) and calls `RealGithubPrApi::with_token`.

**`packages/tddy-daemon-rpc/src/pr_stack/branch_legs.rs`**
- `worktree_leg` returns `BranchWorktree { exists, path, dirty, dirty_paths }` (tracked paths only) from `git worktree list` plus `git status --porcelain` (:60–93).
- `base_sync_through_cache` goes through the process-wide `base_sync_cache::shared()` (:9).

**`packages/tddy-prstack-web/src/useQueryBranch.ts`**
- `POLL_INTERVAL_MS = 5000` (:14).
- `poll()` issues one `client.queryBranch` per active query on `setInterval(poll, 5000)` (:121–144).
- Called from `usePrStackPlan.ts:94` and `ProjectStackView.tsx:159`.

**`packages/tddy-pr-stack/src/pr_insight.rs`**
- `read_pr` (:105) makes 3–4 calls: `get_pr`, `list_reviews`, `list_check_runs(head_sha)`, and optionally `list_pr_files`.
- `read_pr_comments` (:171) makes 3 calls: reviews, review comments (threaded via `threads_of`), and issue comments.
- `pull_number_for_node` (:294): "StackNode carries no PR-number field, so every caller that needs one recovers it from the recorded URL".

**`packages/tddy-daemon-rpc/src/exec_tool/stack_tools.rs` and `stack_tools/{github.rs,ops.rs}`**
- `StackGithubClients::client_for(repo, token)`; `RealStackGithubClients` builds `RealGithubPrApi::with_token` (:60–68).
- `GithubSource::client()` (`github.rs:45`) resolves the token as `local_gh_github_token` or vault `acting_identity(&project.accounts)`.
- `ops.rs:78–110`: `ReadPr` → `read_pr`, `PrComments` → `read_pr_comments`, `SearchPrs`.
- `status` (:182) → `refresh_internal_statuses` → `assemble_views` → `gh.get_open_pr(branch)` for every node with a branch (`tddy-pr-stack/src/assess.rs:225`).

**`packages/tddy-pr-stack/src/assess.rs:339`**
- `RealGithubPrApi::asking_the_session_host(&github_owner_repo)`: the workflow-recipe path asks the session host for a token over the toolcall socket.

**Other one-shot REST callers**
- `packages/tddy-tools/src/github_credential.rs` and `server.rs:682,864`: MCP `github_create_pull_request` / `github_update_pull_request` over curl.
- `packages/tddy-workflow-recipes/src/merge_pr/github.rs:44,87`: its own curl to `api.github.com/.../pulls` and `/merge`.
- `packages/tddy-workflow-recipes/src/orchestrate_pr_stack/actions.rs:107,171`: `RealGithubPrApi::with_token`.

**`packages/tddy-daemon-auth/src/github_pr_credentials.rs`**
- `PrLookup { Empty (stub mode), Perform(token), Unavailable }` (:16).
- `retained_github_token(vaults, login)` (:61) reads the vault entry `GITHUB_PROVIDER / login`.

**`packages/tddy-agent-launch/src/session_acting_identity.rs`**
- `project_github_token(...)` (:108) and `local_gh_github_token(github_cli, os_user, git_url)` (:294), the latter via `gh auth token`.

**Investigate branch** (`AGENTS.md` on `origin/investigate/github-api-rate-limit`, final commit `684c3cfa7`). Text added:

> "**Watch GitHub sparingly.** Every agent shares the operator's 5,000 REST requests/hour. To wait for CI, run `scripts/ci-status.sh --watch` in the background under `timeout`, never `gh run watch` (it polls every 3 s). Keep one watcher per PR. A foreground command that times out keeps running, so do not start another one."

The earlier, longer version (`4ebc40364`) adds:
- the incident: "On 2026-11-21 one session used the whole hour in 32 minutes with stacked `gh run watch` processes";
- "~50 REST requests a minute" per watcher;
- "Superset, Coderplex, Warp, and the daemon use it too";
- "Prefer GraphQL-backed reads (`gh pr view --json …`, `gh pr checks`), which use a separate bucket";
- "On `API rate limit exceeded`, stop all GitHub polling … `gh api rate_limit` (free) to read `core.reset`".

#### B. Session inspector

**`packages/tddy-session-web/src/InspectorTabs.tsx`** (153 lines)
- Tabs in strip order, with test ids:

| Tab | Test id | Shown when |
|---|---|---|
| Details | `sessions-inspector-tab-details` | always |
| Activity | `sessions-inspector-tab-activity` | `activityAvailable` |
| Tools | `sessions-inspector-tab-tools` | always |
| Agents | `sessions-inspector-tab-agents` | always |
| Usage | `sessions-inspector-tab-usage` | always |
| Worktree | `sessions-inspector-tab-worktree` | always |
| Files | `sessions-inspector-tab-files` | always |
| VNC | `sessions-inspector-tab-vnc` | `mediaAvailable` |
| Screen Sharing | `sessions-inspector-tab-screen-sharing` | `mediaAvailable` |

- Each tab is a hand-written `<button>` block; there is no tab array.

**`packages/tddy-client-web/src/appRoutes.ts:239–286`**
- `INSPECTOR_TAB_NAMES = ["details","activity","tools","agents","usage","worktree","files","vnc","screen-sharing"]`.
- `DORMANT_INSPECTOR_TAB_NAMES = ["activity"]`; `MEDIA_INSPECTOR_TAB_NAMES = ["vnc","screen-sharing"]`.
- The list lives here "so the `inspector=` param can be validated from a pure module".

**`packages/tddy-session-web/src/SessionInspectorDrawer.tsx`** (453 lines)
- Default tab (:137–138): `requestedTab = isInspectorTabName(tabParam) ? tabParam : "details"`, where `tabParam` is the `?inspector=` URL param.
- Degrade to `"details"` (:164–168) when a media tab is requested without media, or a dormant tab is requested on a live session.
- Content is dispatched with a ternary chain. The `worktree` arm (:298–307) renders `<SessionWorktreeTab client={worktreeClient} sessionToken projectId sessionId repoPath={session.repoPath}/>`.
- Props include `worktreeClient?: Client<typeof WorktreeService>` (:62). There is no PrStack client prop.

**`packages/tddy-session-web/src/useSessionsDrawerInspector.ts`**
- The `?inspector=` param's presence means the drawer is open; `full=1` means expanded.
- An unknown tab is normalised to `"details"` with `replace` (:56–60).

**`packages/tddy-session-web/src/inspectorState.ts`**
- `defaultInspectorOpen()` returns `false`; on `select` the drawer is closed.

**`packages/tddy-workspace-web/src/worktrees/SessionWorktreeTab.tsx`** (207 lines; exported from tddy-workspace-web)
- Shows Disk / Last calculated / Branch / Changed files.
- Actions: Refresh (`calculateWorktreeSize`), Clear → Confirm (`cleanWorktree`, i.e. `git clean -fdx`), Delete → Confirm (`removeWorktree`), and Restore when missing (`restoreSessionWorktree`).
- Test ids `session-worktree-{tab,error,size,last-calculated,branch,changed,refresh,clear,clear-confirm,delete,delete-confirm,missing,restore}`.
- It shows no dirty-file list, no ahead/behind, and no unpushed/unpulled commits.

**`packages/tddy-workspace-web/src/worktrees/useSessionWorktreeStats.ts`**
- Streams `client.streamWorktreeStats({ projectId, recalculateAll:false })` for the whole project and picks the row whose `path === repoPath`. It is a per-mount stream.

**`packages/tddy-service/proto/worktree.proto`**
- `WorktreeService`: ListWorktreesForProject, RemoveWorktree, StreamWorktreeStats, CalculateWorktreeSize, CleanWorktree, RestoreSessionWorktree, ListWorktreeDirectory, ReadWorktreeFile, StreamReadWorktreeFile.
- `WorktreeRow { path, branch_label, disk_bytes, changed_files, lines_added, lines_removed, updated_at_unix_ms, stale, size_status, size_calculated_at_unix_ms }`. There are no ahead/behind or upstream fields.

**`packages/tddy-service/proto/session.proto`**
- `rpc GetWorktreeSnapshot` (:62); `GetWorktreeSnapshotResponse { head_commit, branch, changed_paths, changed_files, lines_added, lines_removed, untracked_files, attachments }` (:190). There is no ahead/behind.
- `SessionEntry` fields:
  - `branch = 28` ("from Changeset.branch … Lets the PR-Stack view resolve …");
  - `session_role = 35`;
  - `orchestrator_session_id = 21`;
  - `stack_plan_json = 23`;
  - `stack_id = 38`;
  - `repo_path = 4`.
  - There is no PR number, PR URL or upstream status field.

**`packages/tddy-service/proto/worktree_activity.proto`**
- Message-only broadcast on topic `worktree.activity`: `WorktreeActivityEvent { kind: COMMIT|FILES_CHANGED, seq, head_commit, changed_files, lines_added, lines_removed }`.

**`packages/tddy-worktree-service/src/worktree_measurement.rs`**
- `snapshot_worktree` runs `git diff --numstat HEAD`, `rev-parse HEAD`, `rev-parse --abbrev-ref HEAD` and an untracked count. It is used by the session-room poll (`session_room.poll_interval_ms` = 2 s) and by `GetWorktreeSnapshot`.

**`packages/tddy-worktree-service/src/base_freshness.rs`**
- `BaseFreshness { trunk_ref, home_commit, remote_tip, ahead, behind }` from `git rev-list --left-right --count HEAD...{trunk_ref}` after a fetch.
- `CUT_BASE_BEHIND_ARGS = ["rev-list","--count","HEAD..@{upstream}"]` (:121). This is the only `@{upstream}`-relative count found.

**`packages/tddy-session-worktree/src/base_sync.rs`**
- `count_divergence(base...head)` gives `(behind, ahead)` against a base ref, not the upstream. It is a read-only, `merge-tree`-based conflict probe.

**`packages/tddy-changeset/src/changeset/model.rs`**
- `Changeset.branch: Option<String>` (:60).
- `workflow.github_pr_status: Option<GithubPrStatus { phase, url, error }>` (:211–225), written only by the tddy-presenter post-workflow path.
- `StackNode.pr_status: Option<GithubPrStatus>` (`changeset/stack.rs:47`).

**Cypress, page objects and docs**
- `packages/tddy-web/cypress/component/SessionWorktreeTabAcceptance.cy.tsx` has 6 tests (disk usage/branch, Calculating, Refresh, Clear confirm, Delete confirm, Restore).
- `packages/tddy-session-web/cypress/component/SessionInspectorUrlStateAcceptance.cy.tsx:91–102` asserts the `inspector=worktree` param.
- `SessionInspectorAcceptance.cy.tsx:286` asserts "Details is selected by default".
- Page objects: `packages/{tddy-session-web,tddy-web}/cypress/support/pages/sessionWorktreeTabPage.ts`.
- Test ids: `packages/tddy-session-web/cypress/support/testIds.ts:149–262, 488`.
- Stories: no Storybook story exists for the inspector or the Worktree tab. The closest is `packages/tddy-storybook/src/tddy-prstack-web/PlannedPrPanel.stories.tsx`.
- `docs/ft/web/session-worktree-inspector.md` describes `ListWorktreesForProject` and a 10-minute refresh. The code uses `StreamWorktreeStats` instead, so the doc has drifted.
- Existing PR UI lives in `tddy-prstack-web` (`PlannedPrRow.tsx`, `PlannedPrRowBadges.tsx`, `PlannedPrRowDetails.tsx`, `PlannedPrList.tsx`), which reads `BranchResolution.pr` (`PrStatusView`). No check-run or comment UI exists in any web package.

#### C. Role

**`packages/tddy-changeset/src/session_metadata.rs`**
- `SessionMetadata.session_role: Option<String>` (:138).
- `PROJECT_HOME_ROLE = "project-home"` (:177), `ORCHESTRATOR_ROLE = "orchestrator"` (:183).
- `is_orchestrator_role(role) = role == PROJECT_HOME_ROLE || role == ORCHESTRATOR_ROLE` (:190). Its doc says it is "The one decision point every 'is this an orchestrator?' gate asks".
- Exposed on the wire as `SessionEntry.session_role` (35).
- Web constants: `packages/tddy-session-web/src/utils/projectHomeGroups.ts:38,41` and `packages/tddy-session-create-web/src/CreateSessionOrchestratorField.tsx:2`.

### 3. Grep / glob patterns and notable hits

| Pattern | Scope | Notable hits |
|---|---|---|
| `api.github.com\|get_pr_by_head\|GithubPrInsightApi\|list_check_runs\|RealGithubPrApi\|TrunkPoller\|RestTrunkFeed\|GhCliTrunkFeed` | packages, .rs/.ts/.tsx | 33 files; production ones: `tddy-daemon-rpc/src/exec_tool/stack_tools.rs`, `pr_stack/{ports,pr_status}.rs`, `tddy-pr-stack/src/{assess,pr_insight,stack_ops/adopt}.rs`, `tddy-session-activity/src/trunk_watch.rs`, `svc_trunk_watch_wiring.rs`, `tddy-tools/src/{github_credential,server}.rs`, `tddy-workflow-recipes/src/{merge_pr/github,orchestrate_pr_stack/actions,orchestrate_pr_stack/mod}.rs`. No TS file calls GitHub directly. |
| `Command::new("gh")\|statusCheckRollup\|graphql\|"api","-i"` | Rust | Only `tddy-host-service/src/github_cli.rs:171`. No GraphQL anywhere. |
| `"gh"` (non-test Rust) | packages | `host_tooling.rs:349` (probe), `github_cli.rs:211`, `tddy-toolcall/src/toolcall/github_token_client.rs:70` (`find_on_path("gh")` for local-gh tokens). |
| `is_orchestrator_role\|ORCHESTRATOR_ROLE\|PROJECT_HOME_ROLE\|session_role` | daemon crates | See the role-check list in §4C. |
| `sessionRole\|"orchestrator"\|"project-home"` | TS | `SessionDrawerItem.tsx:120`, `SessionWithActivitySidebar.tsx:147`, `useChildSessions.ts:32-33`, `createSessionRequest.ts:255`, `orchestratorStartRequest.ts:44`. |
| `session_tool_role(` | daemon-rpc, session-lifecycle | `base_freshness.rs:79`, `session_messages.rs:136`, `session_files.rs:111`, `project_home.rs:299,310`, `project_home/caller.rs:28,64`, `session_messages/inbox_watch.rs:53`, `svc_list_exec_tools_wiring.rs:107,117`. |
| `InspectorTabName\|isDormantInspectorTabName\|isMediaInspectorTabName` | TS | `tddy-client-web/src/appRoutes.ts:239-286`, re-exported from `tddy-web/src/routing/appRoutes.ts:120-133`. |
| `SessionWorktreeTab` | TS | `tddy-workspace-web/src/worktrees/SessionWorktreeTab.tsx`, used in `SessionInspectorDrawer.tsx:24,300`, spec in `tddy-web/cypress/component/SessionWorktreeTabAcceptance.cy.tsx`. |
| `snapshot_worktree\|left-right\|ahead\|behind\|unpushed\|unpulled` | Rust | `worktree_measurement.rs:29`, `base_freshness.rs:91`, `base_sync.rs:185-213`, `remote_branch_push_gating_tests.rs:5` (a test fixture "unpushed branch"). |
| `queryBranch\|getPrStatus\|POLL\|setInterval` | web src | `useQueryBranch.ts:14,144` (5 s); `ProjectsContent.tsx:17` (5 s); `DaemonSettingsScreen.tsx:26` (5 s); `PathAccessRequests.tsx:22` (3 s). None of the last three call GitHub. |
| `github_pr_status` | Rust | Writers: `tddy-presenter/src/post_workflow.rs`, `presenter/workflow_runner.rs:181`. Reader: `tddy-changeset/src/changeset/merge.rs:96`. |
| `rate.limit\|x-poll\|etag\|trunk.watch\|QueryBranch\|pr.status\|inspector\|session-web\|check.runs` | `docs/dev/todo`, `packages/*/docs/code-issues` | 23 files; the relevant ones are in §4D. |
| `Claimed by` | same | Only unrelated `2026-09-19-test-binary-rs-is-950-production-lines.md:5`. None of the relevant entries carry a claim. |
| `WATCH_KINDS\|WatchKind\|with_trunk_watch\|spawn_trunk_watch` | Rust | `project_home_watch_tools.rs:29` (8 kinds), `session_watches/matcher.rs:9-41`, `tddy-daemon/src/runtime/mount.rs:96,107`, `project_home_watches.rs:128` (`own_trunk_watch = kinds == ["trunk_commit"]`). |

### 4. Findings

#### A. GitHub access today

**Transports.** There are three, and they share nothing:
1. **reqwest** (`RestTrunkFeed`): a new `Client` per call, conditional with ETag.
2. **`gh api -i` as the OS user** (`GhCliTrunkFeed` via `GithubCli::api_with_headers`): conditional with ETag.
3. **`curl` subprocess with temp files** (`github_rest_common`, used by `RealGithubPrApi`, plus a separate curl in `merge_pr/github.rs`): unconditional, discards response headers, no ETag, no `X-Poll-Interval`, no `X-RateLimit-*`.

Rate limits are not detected anywhere; they surface only as an `Unavailable(...)` string or a `WorkflowError`. There is no GraphQL.

**Auth / token sources.** There are four, and they differ by path:
- **Project account from the vault**: `SessionAccountAccess::acting_identity(project.accounts)`. Used by the trunk watch REST feed (re-resolved per poll), the stack tools, and `RepointPlannedPr`.
- **Local gh**: `gh auth token --hostname` as the OS user, or `gh api` directly. Used by the trunk watch for `local_gh` projects, the stack tools, and repoint.
- **Login-keyed retained token**: `retained_github_token(vaults, github_login)`. Used by `pr_status_for_caller`, i.e. GetPrStatus, QueryBranch and PullBaseIntoBranch. The repoint code calls this "the credential this stack retires" (`ports.rs:556`).
- **Session host over the toolcall socket**: `asking_the_session_host` / `github_token_for_session`. Used by tddy-tools MCP PR tools and the `assess.rs` workflow path.

**Caching.** None of the GitHub reads are cached. The `TrunkPoller` keeps one ETag and the last tip, in memory, per project. `base_sync_cache::shared()` caches only the git `merge-tree` comparison.

**Every GitHub caller, with interval, trigger and sharing:**

| # | Caller | Request | Interval | Triggered by | Shared? |
|---|---|---|---|---|---|
| 1 | Trunk watch (`trunk_watch.rs` + `svc_trunk_watch_wiring.rs`) | `GET repos/{o}/{n}/commits?sha=&per_page=100`, conditional | `X-Poll-Interval` or 60 s; 60 s back-off on error; unconditional every 15 min; driver ticks every 1–5 s | orchestrator-role start/resume (`cli_branch_starts.rs:145`, `svc_resume_session.rs:85`) | One poller per project; fan-out per subscriber; dropped with the last live subscriber |
| 2 | PR-stack live status (`useQueryBranch` → `QueryBranch` → `pr_status_for_caller` → `get_pr_by_head`) | `GET pulls?state=all&head=owner:branch`, unconditional | 5 s × every rendered branch (own plus base branches, deduplicated) | each mounted `PrStackScreen` (`usePrStackPlan.ts:94`) or `ProjectStackView` (`ProjectStackView.tsx:159`) | Per subscriber: every browser tab or view polls on its own; no server cache |
| 3 | `GetPrStatus` RPC | same as #2 | on demand | web/client call; the older `usePrStatus` hook was removed | Per call |
| 4 | `PullBaseIntoBranch` | same as #2, once after the pull | per action | operator | Per call |
| 5 | Stack tool `status` (`ops.rs:182` → `assemble_views` → `get_open_pr`) | `GET pulls?state=open&head=` per node with a branch | each agent tool call | orchestrator / stack-node agent | Per call |
| 6 | Stack tools `read_pr` / `pr_comments` / `search_prs` | 3–4 GETs (pull, reviews, check-runs, optional files) / 3 GETs (reviews, review comments, issue comments) / search | per agent tool call | agents | Per call, uncached |
| 7 | Stack mutations (adopt, sync, repoint, merge, close, create, patch title/body) | `get_pr`, PATCH, PUT, POST | per action | agent or operator | — |
| 8 | Workflow recipes: `orchestrate_pr_stack` (`assess.rs:339`, `actions.rs`) and `merge_pr/github.rs` | `get_open_pr` per node; merge | per workflow step | workflow | — |
| 9 | tddy-tools MCP `github_create_pull_request` / `github_update_pull_request` | POST / PATCH | per tool call | agent | — |

**CI checks.** Check runs are read only by `read_pr` (`GET commits/{sha}/check-runs`, per agent call). Nothing in the daemon or web polls checks. `CheckRun` holds only `{name, conclusion}`.

**Recorded volume.** `docs/ft/coder/pr-stack-live-status.md:341` records the old duplicate-hook cost (≈1440 requests/hour/branch). The 2026-07-26 todo puts the current cost at "~2 calls/second" for a ten-node stack.

#### B. Session inspector

- **Tabs and default.** The tab name lives in the `?inspector=` URL param; an absent or unknown value means `details`. The drawer is closed by default on selection.
- **Adding a tab touches three places:** `INSPECTOR_TAB_NAMES` (tddy-client-web), a button in `InspectorTabs.tsx`, and an arm in the ternary chain in `SessionInspectorDrawer.tsx`. Conditional tabs use the `DORMANT_*` / `MEDIA_*` gating pattern.
- **The Worktree tab already exists.** Tab name `worktree`, test id `sessions-inspector-tab-worktree`, component `SessionWorktreeTab` in tddy-workspace-web. It shows disk size, branch and the changed-file count, with Refresh / Clear / Delete / Restore. It does not list dirty files and does not show unpushed or unpulled commits.
- **Worktree state data already available:**
  - `WorktreeRow` (changed_files, ±lines);
  - `GetWorktreeSnapshot` (changed_paths, untracked_files, head_commit, branch);
  - `QueryBranch.worktree` (`dirty_paths`, tracked only);
  - `QueryBranch.base_sync` (ahead/behind against a named base ref, conflicts);
  - `BaseFreshness` (ahead/behind against the trunk, after a fetch; orchestrator home only, via tool);
  - `CutBase.behind` (`HEAD..@{upstream}`).
- **Not available today:** no RPC returns ahead/behind against the branch's own upstream (unpushed or unpulled commits) or a dirty-file list for an arbitrary session.
- **How a session knows its PR.** Only through `SessionEntry.branch` (from `Changeset.branch`). There is no PR-number field anywhere. `Changeset.workflow.github_pr_status {phase,url}` exists only for tddy-coder post-workflow sessions. `StackNode.pr_status.url` is parsed for a number (`pull_number_for_node`). A PR is otherwise found by head-branch lookup (`get_pr_by_head`).
- **No PR panel exists.** There is no check-run or comment UI in any web package. PR state UI exists only in tddy-prstack-web row badges and details, driven by `BranchResolution.pr` (`PrStatusView { exists, number, url, state, unavailable, unavailable_reason }`).

#### C. Role checks to avoid

The role comes from `SessionMetadata.session_role` in `.session.yaml` (on the wire as `SessionEntry.session_role`), checked through `tddy_changeset::session_metadata::is_orchestrator_role` (project-home or orchestrator).

**Rust:**
1. `tddy-agent-launch/src/cli_branch_starts.rs:145`: trunk-watch subscription at start.
2. `tddy-agent-launch/src/svc_resume_session.rs:81-102`: trunk-watch subscription at resume.
3. `tddy-agent-launch/src/back_channel.rs:230`: `peer_name_args`.
4. `tddy-agent-launch/src/cli_agent_steps/conversation.rs:291`: `refuse_a_role_the_agent_cannot_hold`.
5. `tddy-agent-launch/src/conversation_spawn.rs:25-29`: `session_tool_role` returns `ProjectHome`. It is used by:
   - `svc_list_exec_tools_wiring.rs:107,117` (offered catalog);
   - `base_freshness.rs:79`;
   - `session_files.rs:111`;
   - `session_messages.rs:136`;
   - `session_messages/inbox_watch.rs:53`;
   - `project_home.rs:299,310`;
   - `project_home/caller.rs:28,64`.
6. `tddy-agent-launch/src/harness_profile_request.rs:164,169`: `admit_a_requested_role`.
7. `tddy-agent-launch/src/orchestrator_start.rs:30,70,102`: role admission, `place_an_orchestrator`, and `fast_forward_a_resumed_orchestrators_home`.
8. `tddy-agent-launch/src/project_home_launch.rs:212,277`: `main_plan` and the opening prompt.
9. `tddy-agent-launch/src/project_home_idle.rs:93`: auto-stop (project-home only).
10. `tddy-daemon-rpc/src/exec_tool/role_guard.rs:52-58`: read-only refusal for orchestrator roles.
11. `tddy-daemon-rpc/src/exec_tool/orchestrator_reach.rs:84-90`: `is_orchestrator_session`.
12. `tddy-daemon-rpc/src/exec_tool/lsp_roots.rs:202-206`.
13. `tddy-daemon-rpc/src/exec_tool/project_home_remote.rs:148`.
14. `tddy-daemon-rpc/src/exec_tool/stack_tools.rs` `stack_caller_of`, which leads to `tddy-tool-engine/src/stack_tools.rs:129` (`stack_caller_role`) and `stack_tools/caller_records.rs:171` (`forwarded_stack_caller_role`).
15. `tddy-tool-engine/src/project_home_git_tools.rs:170-171`: refuses another orchestrator's worktree as a target.
16. `tddy-daemon-rpc/src/project/project_sessions.rs:38,60`: delete blocking and project-home stop (project-home only).
17. `tddy-daemon-rpc/src/exec_tool/project_home_watches.rs:128`: the self-watch admits only `trunk_commit` (a kind check, not a role check, but tied to the orchestrator use).

**Web:**
18. `tddy-session-web/src/SessionDrawerItem.tsx:120`: orchestrator badge.
19. `tddy-session-web/src/SessionWithActivitySidebar.tsx:147`: `PathAccessRequests` is shown only for orchestrator or project-home.
20. `tddy-session-web/src/useChildSessions.ts:32-33`: no child tabs for orchestrators.

No inspector tab is gated on role today.

#### D. Backlog and code issues

None of these carries a `**Claimed by:**` line. The code-issue records say "unclaimed" explicitly.

| Path | Title | Gist |
|---|---|---|
| `docs/dev/todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md` | PR-Stack — status polling and stack hygiene | One GitHub lookup per rendered branch every 5 s; "Batch the resolution into one call per stack, cache per branch with ETags, or back off when the response is unchanged". Also: dangling `session_id`, and `origin/<branch>` freshness depends on the last fetch. |
| `docs/dev/todo/2026-08-30-pr-stack-querybranch-s-local-only-legs.md` | QueryBranch's local-only legs | The worktree and remote legs answer only for the daemon asked; fixing it means a per-branch fan-out on a 5 s poll, or an `unavailable` discriminator. |
| `docs/dev/todo/2026-11-21-trunk-watch-reads-github-com-only.md` | The trunk watch reads github.com only | Enterprise is unsupported: `owner_repo_from_remote_url` is github.com-only and `RestTrunkFeed` is hard-wired to api.github.com. Fix: derive the API base from the host. |
| `docs/dev/todo/2026-07-30-pr-stack-full-control-follow-ups.md` | PR-Stack — full control follow-ups | No testable transport seam for `RealGithubPrApi` (curl; "Adding a base-URL override plus a real HTTP client"); review-thread resolution needs GraphQL ("the first GraphQL call in the repository"); `pr_search` neither paginates nor returns branches; no gRPC/web surface for update, adopt and similar. |
| `docs/dev/todo/2026-10-23-session-status-streams-do-not-reconnect.md` | Session status streams do not reconnect | Activity, usage and notification streams show an error but never re-subscribe; proposes a shared reconnect-with-backoff helper. This applies to any new inspector stream. |
| `docs/dev/todo/2026-11-07-session-web-specs-do-not-typecheck.md` | tddy-session-web's drawer-driven Cypress specs do not typecheck | 181 TS errors under `cypress/`, including the `SessionInspector*` and `PrStack*` specs; `src/` is clean. |
| `docs/dev/todo/2026-08-01-tddy-web-inactive-session-activities-follow-ups.md` | inactive session activities follow-ups | Removing inspector docking has no direct test (360px overlay). |
| `docs/dev/todo/2026-07-04-tddy-github-tddy-daemon.md` | tddy-github / tddy-daemon | HMAC signer and token revocation; touches tddy-github only, not polling. |
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md` | oversized-file: pr_stack/ports.rs | 868 lines, budget 500, "unclaimed", accepted as standing debt 2026-11-14. |
| `packages/tddy-github/docs/code-issues/oversized-file-pr-api.md` | oversized-file: pr_api.rs | About 915 production lines; split by trait required; Open. |
| `packages/tddy-github/docs/code-issues/dead-code-github-pr-mock-transport.md` | dead-code: MockGithubTransport | Production-dead; Open. |
| `packages/tddy-workflow-recipes/docs/code-issues/misplaced-tests-github-rest-client.md` | misplaced-tests: the GitHub REST client's acceptance suites | Three test binaries should move to tddy-github; Open. |
| `packages/tddy-session-web/docs/code-issues/oversized-file-session-main-pane.md` | oversized-file: SessionMainPane.tsx | 504 lines, unclaimed, deferred. This is where the inspector is mounted. |

#### Other facts relevant to the plan

- **Event plumbing a central inbox could reuse.**
  - `SessionNotificationBus` (tddy-session-activity) feeds the project notification log, which feeds `WatchKind` matching for watches (`matcher.rs`, 8 kinds, `WATCH_KINDS` in tool-engine) and wakes idle agents (`watch_wake.rs:94`).
  - The trunk watcher publishes `SessionNotificationSource::TrunkCommit` per subscriber. Adding a kind means touching `SessionNotificationSource`, `WatchKind::ALL`, `WATCH_KINDS`, `is_a_sessions_own`, the drawer-stream skip (`streams.rs:69`) and the Telegram decline.
- **Session-room poll.** The room metadata poll runs `snapshot_worktree` every `session_room.poll_interval_ms` (2 s by default; `session_room.rs:317,1489`). It broadcasts `WorktreeActivityEvent` (COMMIT / FILES_CHANGED) on `worktree.activity`. This is already a local "worktree moved" signal that a GitHub inbox could use as an invalidation trigger.

## Exploration 3 — stack store, tools, activities, git helpers

## Discovery dump: per-project PR stack store → per-session work queue (current state, read-only)

Worktree: `/Users/mantasi/Code/tddy-coder/.worktrees/plan-workqueue-planner` (HEAD `97018314f`). The only untracked file is `docs/dev/1-WIP/2026-11-22-workqueue-whole-work-initial-discovery.md`, an 11-line placeholder whose Combined Conclusions still read "(pending)".

Headline facts:
- **Store:** stacks are YAML files keyed by project and stack id, not SQLite and not keyed by session. The record already stores the session that created it.
- **Gating:** stack tools are gated by session role in code. No config or YAML gates them.
- **Git logic:** most of what the plan needs already exists in some form — waves, ahead/behind, merge-tree conflict prediction, rebase, repoint, merge. The gaps are `gh stack` calls and a merge-and-repoint chain run from the daemon.

---

### 1. Sequence (chronological)

1. `git log -3`, `ls packages/` — orientation.
2. Grep `ProjectStackStore` in `*.rs` — the glob form failed in zsh, so I re-ran it scoped to `packages`. Goal: find the definition and its users.
3. `find` over `tddy-pr-stack`, `tddy-prstack-web` and `tddy-session-activity` files, plus `wc` of the tool-engine stack files — package inventory.
4. Grep `struct|impl ProjectStackStore` → definition in `tddy-changeset`.
5. `wc`, `Cargo.toml` and `README` of `tddy-pr-stack` — crate contents.
6. Read `tddy-changeset/src/project_stack_store.rs` — storage backend.
7. Read `tddy-changeset/src/changeset/stack.rs` — `Stack`/`StackNode` model and session→stack resolution.
8. Grep `Changeset` fields (`stack_id`, `orchestrator_session_id`) and `GithubPrStatus`/`PrInternalStatus` in `model.rs`.
9. Read `tddy-pr-stack/src/stack_migration.rs` (head, then remainder) — existing migration.
10. Grep creation call sites: `migrate_session_stacks|ProjectStackStore::new|create_titled|.create(`.
11. `ls`/`wc` of the tool-engine and daemon-rpc stack dirs.
12. Read `tddy-tool-engine/src/stack_tools.rs` in full — tool names, args, roles.
13. Read `tddy-tool-engine/src/stack_git_tools.rs` (lines 1–420) and `stack_tools/defs.rs`.
14. Read `stack_tools/caller_records.rs` (1–200) — caller classification.
15. Grep for registration and gating sites (`stack_tools_offered_to`, `is_stack_tool`, …).
16. Read `daemon-rpc/exec_tool/stack_tools.rs` and `stack_git_tools.rs` — the daemon ports.
17. Read `git_work.rs` and `git_work/node_ops.rs` — merge, close, repoint, set-parents, merge-tree.
18. Read `daemon-rpc/exec_tool/stack_tools/ops.rs` — plan-tool ops and status derivation.
19. Read `tddy-pr-stack/src/stack_ops/mod.rs`; grep signatures in `git_ops.rs`.
20. Grep `merge-tree|merge_base|rev-list|--left-right|ahead_behind|--count` across packages.
21. Grep hit lines in the files from step 20.
22. Read `tddy-session-worktree/src/base_sync.rs` (1–80, 170–270).
23. Read `tddy-worktree-service/src/base_freshness.rs` (1–140) and `base_sync_cache.rs` (1–60).
24. Grep `gh stack|gh-stack|gh_stack` (no production hits); read `tddy-subagent-worktree` `lineage.rs` and `sync.rs`.
25. `ls`/grep the `tddy-workflow-recipes/orchestrate_pr_stack` surface.
26. Read `internal_status.rs`, `transient.rs` (journal) and `bridge.rs` (merge and repoint).
27. Find and read `tddy-service/proto/pr_stack.proto` in full.
28. Read `prstack-web` `package.json`, `README`, `docs/README`; sizes; specs and stories.
29. Read `ProjectStackView.tsx` in full.
30. Grep `ProjectStackView` mount sites and all `data-testid`s in `prstack-web`.
31. Read the `SessionsDrawerDetailPane.tsx` mount, `ProjectStacksRow.tsx` and the `appRoutes` route; grep Storybook.
32. Read `stackPlan.ts`, `useQueryBranch.ts`, `projectStackViewPage.ts`; grep spec titles.
33. Read `daemon-rpc/pr_stack/project_stacks.rs` (head) and list functions in `project_stack_queries.rs` and `ports.rs`.
34. Read `create_project_stack_rpc` and `referencing_sessions_by_stack`; grep `stack_plan_json` in enrichment and proto.
35. Grep `child_recipe` and the public functions of `tddy-pr-stack`.
36. Read `assess.rs` lines 30–150 (`NodeView`, `decide_next_action`).
37. `wc` of the session-activity sources; read `session_watches.rs` (1–200).
38. Read `session_watches/matcher.rs` (the `WatchKind` enum).
39. Read `trunk_watch.rs` (1–80) and the trunk-watch changesets (package-local, then `docs/dev/changesets/2026-11-21-trunk-watch.md`).
40. Grep the `SessionNotificationSource` enum in `tddy-daemon-kernel`.
41. Grep `WATCH_KINDS`; read the `session_messages.rs` head; grep the `send`/`monitor` CLI.
42. Read the `project_home_watch_tools.rs` head and the `tddy-tools` `send_cli` head.
43. Read `exec_tool/project_home.rs` routing (120–145, 280–316), `server.rs` dispatch (340–365) and the ListExecTools jail wiring test.
44. Read the `watch_events.rs` and `watch_wake.rs` heads; grep `is_orchestrator_role`, `session_tool_role` and `SessionToolRole`.
45. Grep `stack` in `project_home_catalogue.rs`, `read_only.rs` and `lib.rs`.
46. `ls docs/dev/todo | wc` (475 entries); grep todo files by topic. The first broad grep was too wide; I then narrowed it by package and file names.
47. Grep `Claimed by` across `docs/dev/todo`.
48. Grep todo entries that name files these nodes would touch.
49. Read todo entries (2026-07-26, 07-30, 08-30, 11-14 retire-spawn, 11-16 watches-web; then 11-14 node-listing, 11-20 oversized, 11-21 base-freshness, 11-21 trunk-github, 08-29 stack-progress; then 11-22 coder, three 08-13 pr-stack entries, 09-12 session-activity tests, 09-19 flake).
50. List the `code-issues` dirs of 14 packages; read the 7 relevant records.
51. Grep `workqueue|work queue` and `waves` in Rust; `ls` the 1-WIP dirs.
52. Read the workqueue WIP placeholder; read `tddy-build/src/graph.rs` `waves()`.
53. Read `last_run_sweep.rs`; grep the `GithubPrApi` trait methods and the `PrRef`/`PrDetail` structs.
54. Grep the message tool names; read the `send_cli` head; list `pr-stacking.md` headings; read its data model, where-a-stack-lives and merge/repoint sections.
55. Grep the `ProjectNotificationRecord` and `SessionNotification` structs.
56. Grep the `TrunkCommit` touch points (files, then exact lines); read the `CreateSessionWatch` admission, the log `wants` and `watch_wake` `wants`.
57. List the stack and watch test suites with test counts.
58. Read the `PlannedPrRowBadges` head; grep `prstack-web` components; read `index.ts`.
59. Read the `usePrStackPlan` head; grep `<PrStackScreen` mounts.
60. Count production lines (up to the first `#[cfg(test)]`) for the files likely to be touched.
61. `ls` the relevant changesets; grep the decisions in `2026-11-12-project-stack-store.md`.
62. Read the `read_only.rs` head; grep stack in `exec_tool_admission.rs`; grep YAML/TOML for tool gating (none found).

---

### 2. Inspected files (why, and the excerpts that mattered)

#### A. Store and model

**`packages/tddy-changeset/src/project_stack_store.rs`** (479 lines; 271 production) — the store definition.
- 1–9 (module doc): `PR stacks as project records: <projects dir>/<project_id>/stacks/<stack_id>.yaml`. "A stack belongs to the project it was planned in, not to the session that planned it… The session that planned it keeps only a reference — `Changeset::stack_id`." Writes are read-modify-write under an in-process mutex per record path, finished by an atomic rename.
- 18–32 `StackRecord`:
  ```rust
  pub struct StackRecord { pub stack_id: String /*UUIDv7*/, pub project_id: String,
      pub created_by_session_id: String, pub created_at_unix_ms: u64,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub title: Option<String>,
      pub stack: Stack }
  ```
- 36–59 `StackStoreError`: `InvalidProjectId`, `InvalidStackId`, `NotFound{project_id,stack_id}`, `Malformed{path,message}`, `Io{path,source}`.
- 64–66: `pub struct ProjectStackStore { projects_dir: PathBuf }`, built as `ProjectStackStore::new(<sessions_base>/projects)`.
- API:
  - `create(project_id, created_by_session_id, stack)` (83)
  - `create_titled(..., title: Option<String>, ...)` (93) — `uuid::Uuid::now_v7()` at 101
  - `read` (116)
  - `update(project_id, stack_id, f: FnOnce(&mut Stack))` (124) — the closure sees only `Stack`, not the record (title is not editable through it)
  - `list(project_id)` (142) — directory scan of `*.yaml`, sorted by `stack_id`
- `stacks_dir` (174) joins `projects_dir/<project_id>/stacks`. `names_one_path_segment` (202) does path validation.
- `with_record_lock` (218): a global `OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>`. In-process only.
- Serialization: `serde_yaml` (252, 259), written via `tddy_session_store::atomic_file::write_atomic` (263).

**`packages/tddy-changeset/src/changeset/stack.rs`** (992 lines; 463 production) — `Stack`/`StackNode` and session→stack resolution.
- 17–23: `pub struct Stack { #[serde(default)] pub version: u32, #[serde(default)] pub nodes: Vec<StackNode> }`
- 26–63 `StackNode` fields:
  - `node_id: String` ("n1")
  - `title`
  - `description` (default)
  - `branch_suggestion: Option<String>`
  - `branch: Option<String>` (the actual branch; the spawn gate)
  - `session_id: Option<String>` (child session once materialized)
  - `parents: Vec<String>` (parent node ids; empty = root; >1 = DAG)
  - `pr_status: Option<GithubPrStatus>` (phase `planned|open|merged|closed|error`)
  - `child_state: Option<WorkflowState>`
  - `internal_status: Option<PrInternalStatus>`
  - `display_order: Option<u32>` (reading order, independent of `parents`)
  - There is **no** PR-number field (the number is derived from the `pr_status.url` or a `get_open_pr(branch)` lookup), **no** `child_recipe`, and no labels, badges, assignee or associated sessions beyond the single `session_id`.
- 66–72: `is_skipped()` = `pr_status.phase == "merged"`.
- 83–132: `topo_order()` (Kahn; returns `ChangesetInvalid` on a cycle; unknown parents are ignored for in-degree). 155–175: `display_order()`. 194–211: `effective_base_refs`. 225–247: `base_ref_for_spawn`.
- 255–289 `SessionStackHome { store, project_id, session_id }`: the session dir must be `<base>/sessions/<id>`; the project id comes from `.session.yaml` (`read_session_metadata(...).project_id`); empty → `SessionMissing`.
- 300–329: `read_session_stack` and `read_stack_of`. The stack resolves through `changeset.stack_id`. A changeset still carrying the legacy `stack` field is refused ("startup migration has not moved it").
- 334–365: `update_stack_atomic`. The first write creates the record with `created_by_session_id = home.session_id` under a global `FIRST_STACK_WRITE` mutex, then writes `changeset.stack_id` (record before changeset).
- 379–393: `link_stack_node_to_child_session`. 402–409: `resolve_stack_node_branch` (falls back to the child changeset's branch). 433–461: `sync_stack_node_from_child` (copies the child's `state.current` and `workflow.github_pr_status`).

**`packages/tddy-changeset/src/changeset/model.rs`**:
- 90: `pub stack: Option<Stack>` (legacy).
- 94: `pub stack_id: Option<String>` (the session→stack reference).
- 99: `pub orchestrator_session_id: Option<String>` (child→orchestrator back-reference).
- 103: `orchestrator_daemon_instance_id`.
- 219–226: `GithubPrStatus { phase, url: Option, error: Option }`.
- 228–238: `PrInternalStatus { kind, note: Option, source }`, where kind is one of `up-to-date|needs-repoint|has-conflicts|ready-to-merge|blocked|merged` and source is `derived|override`.

**`packages/tddy-pr-stack/src/stack_migration.rs`** (170 production lines) — the only migration.
- 45: `migrate_session_stacks(sessions_base, projects_dir) -> MigrationReport`. It iterates `<base>/sessions/*`.
- `migrate_one` (114): skips when there is no `changeset.yaml` or no legacy stack; `NoProject` when `.session.yaml` names no project; keeps an existing `stack_id`; otherwise calls `record_for`. It then writes `stack=None, stack_id=Some`.
- `record_for` (150): idempotency key is `created_by_session_id == session_id` via `store.list(...)`.

**`packages/tddy-daemon/src/runtime/last_run_sweep.rs`** — 29–35: `sweep_what_the_last_run_left` calls `migrate_session_stacks(tddy_data_dir, &tddy_data_dir.join("projects"))` at startup.

**`packages/tddy-pr-stack/Cargo.toml` and `README.md`**:
- Dependencies: `tddy-core`, `tddy-git`, `tddy-github`, `tddy-workflow`, `tddy-rpc`, `tddy-service`.
- "must never depend on `tddy-workflow-recipes` or `tddy-session-lifecycle`".
- Modules (with production sizes):
  - `stack_ops/{order,nodes,repoint,adopt,seed,pull_base}`
  - `docs.rs`
  - `assess.rs`
  - `git_ops.rs` (468)
  - `pr_insight.rs`
  - `rpc.rs` (`PrStackHandler`, `PR_STACK_SERVICE`)
  - `stack_migration.rs`

**`packages/tddy-pr-stack/src/stack_ops/mod.rs`**:
- 45–51: `trait StackHome { read_stack, update_stack }`, implemented for `Path`/`PathBuf` (session-addressed) and `StoredStack<'a>{store, project_id, stack_id}` (85–109, store-addressed).
- 136–149: `append_node_atomic`.
- 163–184: `validate_parents`. 191: `reject_if_cyclic`. 212: `next_free_node_id` (`"n<max+1>"`).
- Re-exports (25–38): `add_planned_pr_node`, `delete_planned_pr_node`, `set_stack_node_parents`, `update_planned_pr_node`, `assign_missing_display_order`, `move_planned_pr_node`, `pull_base_into_node_branch`/`BaseSyncStrategy`, `repoint_planned_pr_node`, `adopt_pr_into_stack`, `sync_node_to_github_pr`, `seed_stack_with_base_session`.
- `nodes.rs:23–26`: `AddPlannedPrInput.child_recipe` — "`StackNode` has no `child_recipe`" (inert).

**`packages/tddy-pr-stack/src/assess.rs`**:
- 36–43: `OrchestratorAction { Spawn, Wait, Merge, MarkFailed, Done }`.
- 47–55: `NodeView { node_id, branch, parent_dep_ids, child_session_id, child_state, child_phase, pr: PrLiveStatus }`.
- 62–145: `decide_next_action(views, autonomous_merge, approved_nodes)` — a readiness-like rule: "Merge an open PR whose deps are merged, subject to the merge gate".
- 148: `effective_base_ref`. 175: `assemble_views`.

#### B. Tools

**`packages/tddy-tool-engine/src/stack_tools.rs`** (463 production lines):
- 9–25 doc table and role rules. 45–73: eleven tool names.
- 89–97: `enum StackCallerRole { Orchestrator { own_stack_id: Option<String> }, StackNode { stack_id } }`.
- 101–105: `StackCaller { session_id, project_id, role }`.
- 128–140: `stack_caller_role` — an orchestrator role wins over a node binding; "a stack the changeset names gives no role by itself".
- 143–150: `is_stack_tool_offered_to` — a node gets `pr_stack_status` and `pr_read` only.
- 169–196: `addressed_stack_id` — an orchestrator's own stack is the default; a named stack wins; a node may name only its own stack.
- 222–266: `enum StackOp` (`Status`, `ReadPr`, `SearchPrs`, `PrComments`, `AddPlanned`, `UpdatePlanned`, `DeletePlanned`, `SetStatus`, `Adopt`).
- 271–290: `trait StackToolPort { daemon_instance_id, list_stacks(project_id), create_stack(caller, title), run_stack_op(scope, op) }`.
- 313–332: `StackToolArgs`, the union of all arguments.

**`packages/tddy-tool-engine/src/stack_tools/defs.rs`** — schemas and descriptions (all take `daemon_instance_id`; all but list/create take `stack_id`):
- `pr_stack_list` `{}`
- `pr_stack_create` `{title?}`
- `pr_stack_status` `{}`
- `pr_read` `{node_id|pull_number, include_files?}`
- `pr_search` `{query?, state? open|closed|merged|all, author?, base?, limit?}`
- `pr_comments` `{node_id|pull_number}`
- `pr_add_planned` `{title*, description*, branch_suggestion?, parents?}`
- `pr_update_planned` `{node_id*, title?, description?, branch_suggestion?, sync_pr?}`
- `pr_delete_planned` `{node_id*}` (reparents children; refuses an open PR)
- `pr_set_status` `{node_id*, kind*, note?}` (override)
- `pr_adopt` `{pull_number*, parents?}`

**`packages/tddy-tool-engine/src/stack_git_tools.rs`** (287 production lines) — "offered to an **orchestrator** alone, never to a node". Six tools (32–47), each taking `node_id*`, `stack_id?` and `daemon_instance_id?` (188–199):
- `pr_merge` — "Merge a stack node's open pull request… mark the node merged. Then pr_repoint its dependents" (an instruction to the agent; nothing chains automatically).
- `pr_close`
- `pr_repoint {new_base?}` — "Rebase… onto its effective base… force-push it with lease… retarget… Refused… while worktree has uncommitted changes or its session is running."
- `pr_set_parents {parents*}`
- `pr_resolve_conflicts {delegate?}`
- `pr_spawn_child {session_type?, model?}`

Supporting types: `enum StackGitOp` (74–103); `trait StackGitToolPort { run_stack_git_op(caller, stack_id, op) }` (108–118).

**`packages/tddy-tool-engine/src/stack_tools/caller_records.rs`** (187 production lines):
- 59–98: `stack_caller_from_records(sessions_base, session_id)` reads `.session.yaml` `session_role`, the changeset's `stack_id` and `orchestrator_session_id`, and the orchestrator's stack record (`node_binding` 111–138: any node with `session_id == caller`).
- 143: `stack_bound_to_session_here`. 167: `forwarded_stack_caller_role`.
- 181: `stack_and_git_tools_offered_to(role)` → what ListExecTools appends.

**`packages/tddy-daemon-rpc/src/exec_tool/stack_tools.rs`**:
- 75–84: `stack_caller_of`.
- 89–117: `run_stack_tool` — no role → `permission_denied`; then `is_stack_tool_offered_to`; then `served_on_target_host` (forward when `daemon_instance_id` names a peer); then `record_call`.
- 177–241 `DaemonStackTools`: `create_stack` uses `create_titled(&caller.project_id, &caller.session_id, title, Stack{version:1,..})`. The comment "Never the caller's own stack: its changeset is not touched" means the creator id is recorded but `Changeset.stack_id` is not written. `run_stack_op` runs on `spawn_blocking`.

**`packages/tddy-daemon-rpc/src/exec_tool/stack_tools/ops.rs`** (337 lines):
- 73–178: op dispatch.
- 182–209 `status`: refresh, then a JSON list of `{node_id,title,branch,session_id,pr_status,internal_status}`.
- 212–240 `refresh_internal_statuses`: `assemble_views(planner_dir = record.created_by_session_id's dir, ...)`, then `derive_internal_status`, then `reconcile_internal_status` (override wins).
- 309–337 `set_status`: `source:"override"`.

**`packages/tddy-daemon-rpc/src/exec_tool/stack_git_tools.rs`** (432 lines):
- 8–24 doc: git runs in the node's own worktree; `merge-tree --write-tree`; the merge journal lives at `<projects>/<project_id>/stacks/<stack_id>.op.json`.
- 59–86: `run_stack_git_tool`.
- 186–237 `delegate_conflicts`: `send_session_message` when the node's session is active, else `resume_session` with a message.
- 239–344 `spawn_child`: requires `branch_suggestion` and no branch; builds `StartSessionArgs{ stack_parent: caller.session_id, stack_parent_daemon_instance_id, stack_node_id, new_branch_name, branch_worktree_intent:"new_branch_from_base", initial_prompt }`; polls `get_session_start` for up to 120 s; then `store.update` sets `node.session_id` and `node.branch`.

**`packages/tddy-daemon-rpc/src/exec_tool/stack_git_tools/git_work.rs`** (430 lines):
- 36–54: `enum GitWork { Merge, Close, Repoint, SetParents, ResolveConflicts }`.
- 68–84: a `git()` helper.
- 86–104: `project_token` (Local `gh` or vault account).
- 156–174: `default_branch` (`effective_integration_base_ref_for_project` → `main_branch_ref`).
- 197–233: `worktree_for` (target record worktree, else `git worktree list --porcelain` by branch).
- 235–254: `dirty_paths` (`git status --porcelain`).
- 256–275: `refuse_dirty_or_running`.
- 278–300: `effective_base` (first non-merged parent with a branch, else `remote/default`).
- 302–320: `set_phase`.
- 322–337: `pull_number` (`gh.get_open_pr(branch)` → `pull_number_for_node`).
- 339–390 `realign`: `merge_base`, `tddy_pr_stack::git_ops::rebase_onto(repo_root, base_ref, old_base, branch)`, `run_git_op(Push{force_with_lease:true})`, `patch_pr_base`.
- 411–428: journal at `stacks/<stack_id>.op.json` (`StackOpJournal` from the recipes crate).

**`.../git_work/node_ops.rs`** (217 lines):
- 17–42 `merge_node`: journal Planned → `gh.merge_pr(number)` → `PrMerged{sha}` → `set_phase("merged")` → remove journal. **Dependents are recorded in the journal but never repointed.**
- 54–81: `repoint_node`. 83–138: `set_parents` (validate, then `topo_order`, refuse dirty, store update, realign).
- 140–175 `resolve_conflicts`: sets or clears `has-conflicts` (`source:"override"`).
- 179–217 `merge_tree_conflicts`:
  ```rust
  .args(["merge-tree","--write-tree","--name-only", branch, base_ref])
  ```
  Exit 0 = clean, 1 = conflicts (paths parsed after line 1), anything else is an error.

**Gating and registration:**
- `exec_tool/project_home.rs:128–139`, `run_locally`: `if is_stack_git_tool → run_stack_git_tool; if is_stack_tool → run_stack_tool`.
- 307–315 (ListExecTools): "A project-home session's catalogue already carries the stack tools; a session bound to a node of a stack is offered the two reads" → `stack_and_git_tools_offered_to(caller.role)` for non-ProjectHome roles.
- `project_home_catalogue.rs:195–196`: `.chain(stack_tool_defs()).chain(stack_git_tool_defs())`. 131–140: `enum SessionToolRole { ProjectHome, ConversationSpawner, Ordinary }`.
- `tddy-agent-launch/src/conversation_spawn.rs:21–40` `session_tool_role`: `is_orchestrator_role(session_role)` → ProjectHome.
- `tddy-changeset/src/session_metadata.rs:190`: `is_orchestrator_role(role) = role == PROJECT_HOME_ROLE || role == ORCHESTRATOR_ROLE`.
- `read_only.rs:24–36`: `is_project_home_allowed_tool` includes stack and stack-git tools (read-only orchestrators may call them).
- `tddy-tools/src/server.rs:354–359`: stack and git tools → `dispatch_dynamic_tool` (forwarded to the daemon).
- `tddy-tools/src/exec_tool_admission.rs:73–74, 91–94`: `stack_remote_tool_defs()`.
- `tddy-session-lifecycle/.../svc_list_exec_tools_wiring.rs:14, 248`: jail listing uses the same `caller_records`.

#### Recipe-side (legacy) logic, `tddy-workflow-recipes/src/orchestrate_pr_stack/`
- `internal_status.rs:36–68`, `derive_internal_status`: merged → `merged`; open with base ≠ effective → `needs-repoint`; all parents merged → `ready-to-merge`; else `up-to-date`. 71–79: `reconcile_internal_status` (override wins).
- `transient.rs:8–22`: `MergePhase { Planned, PrMerged{sha}, RepointingDependent{idx}, Done }` and `StackOpJournal { op_id, merged_node_id, merge_phase, dependents }`; journal at `<session>/.workflow/stack-op.json`.
- `bridge.rs:51–106`: `execute_stack_merge` (session-addressed). 127–(~250): `execute_stack_repoint`, which walks dependents, rebases onto the default branch, force-pushes, and patches the base. This is a **propagation engine prototype** — session-addressed, run from the recipe, never from the daemon tools.

#### E. Git helpers
- **`packages/tddy-session-worktree/src/base_sync.rs`** (302 lines; also re-exported as `tddy_core::base_sync`):
  - Doc 1–26: no fetch, no index or working-tree mutation.
  - `BaseSyncRefs{base_ref,base_sha,head_ref,head_sha}` (40–47).
  - `BranchBaseSync{base_branch,base_ref,head_ref,behind_count,ahead_count,has_conflicts,conflicted_paths}` (51–66).
  - `branch_base_sync(repo_root, branch, base_branch)` (70–77).
  - `count_divergence` (191–219): `git rev-list --left-right --count base...head`.
  - `conflicting_paths` (228–270): `git merge-tree --write-tree --name-only -z base head`, NUL-parsed.
- **`packages/tddy-worktree-service/src/base_sync_cache.rs`**: cache keyed `(repo_root, base_ref, base_sha, head_ref, head_sha)`, no TTL, capacity 512, failures cached.
- **`packages/tddy-worktree-service/src/base_freshness.rs`**:
  - `BaseFreshness{trunk_ref,home_commit,remote_tip,ahead,behind,fetched_at_unix_ms}`.
  - `base_freshness()` fetches, then `rev-list --left-right --count HEAD...trunk`.
  - `refresh_home` is ff-only.
  - `CUT_BASE_BEHIND_ARGS = ["rev-list","--count","HEAD..@{upstream}"]` (125).
- **`packages/tddy-pr-stack/src/git_ops.rs`** (468 production lines):
  - `local_branch_exists` (22)
  - `rebase_onto(repo_root,new_base,old_base,branch)` (42) — runs in the worktree holding the branch; refuses dirty
  - `force_push_with_lease(repo,remote,branch,expected_sha)` (91)
  - `merge_base` (118)
  - `SyncOutcome` (135)
  - `fetch_ref` (151)
  - `worktree_is_clean` (173) — tracked files only
  - `commit_all_tracked` (212)
  - `merge_ref_into_worktree` (278)
  - `rebase_branch_onto_ref` (308)
  - `push_branch` (335)
  - `head_sha` (351)
  - `ref_sha` (364)
  - `unmerged_paths` (444)
  - `build_integration_ref` (`stack-int/<node_id>`, 460)
- **`packages/tddy-subagent-worktree/src/sync.rs:114–140`**: `merge_tree(base, ours, theirs)` with an explicit `--merge-base`; `parse_merge_tree` (211).
- **`packages/tddy-subagent-worktree/src/lineage.rs`**: `rev-list --first-parent --no-merges --reverse base..branch` (subagent commits — an outbox-like listing).
- **`tddy-workflow-recipes/src/review/git_context.rs:62`**: `git_merge_base(HEAD, other)`.
- **`packages/tddy-build/src/graph.rs:123–161`**: `waves()` via `kahn_waves(nodes.len(), &edges)`. This is the only "waves" implementation in the repo; it lives in an unrelated crate.
- No `gh stack` invocation anywhere in code.

#### C. Web
- **`packages/tddy-prstack-web/src/ProjectStackView.tsx`** (448 lines):
  - Props: `address: {projectId, stackId, daemonInstanceId}`, `prStackClient`, `sessionToken`, `sessions`, `sessionMetadataBySessionId`, `defaultBranch`, `defaultRemote`, `onOpenSession`, `onChildSessionStarted`, plus the start-dialog clients.
  - 127–144: loads **once** per address via `getProjectStack` (no plan polling).
  - 159–164: `useQueryBranch(prStackClient, token, {stack: address}, branchQueries)` polls every 5 s.
  - Handlers call `addPlannedPr`, `repointPlannedPr`, `reorderPlannedPr`, `pullBaseIntoBranch`, `createProjectStack` (title `""`), each by address.
  - Test ids: `project-stack-view`, `data-stack-id`, `project-stack-new-stack-btn`, `pr-stack-add-planned-pr-btn`. Renders `AddPlannedPrForm`, `PlannedPrList`, `DirtyWorktreeDialog`, `CreateSessionDialog`.
- **`src/stackPlan.ts`**: `StackNode` TS shape; parses the snake_case wire format (includes `child_recipe` defaulting to `"tdd"` and `display_order`); `topoSortStackNodes`.
- **`src/useQueryBranch.ts`**: `POLL_INTERVAL_MS = 5000`, ticketed writes, target `{sessionId}|{stack}`.
- **`src/PlannedPrRowBadges.tsx`**: `INTERNAL_STATUS_BADGE_CLASSES` for `needs-repoint`, `has-conflicts`, `ready-to-merge`, `blocked` — the existing badge mechanism (no custom labels).
- Components: `PlannedPrList` (233), `PlannedPrRow` (380), `PlannedPrRowDetails` (211), `PrStackScreen` (395, session-addressed via `usePrStackPlan`), `AddPlannedPrForm`, `DirtyWorktreeDialog`.
- Exports (`index.ts`): `PrStackScreen`, `PrStackChat`, `PlannedPrList`, `ProjectStackView`, `PlannedPrPanel`, `WorkflowChatScreen`.
- Per-node test ids (prefix `pr-stack-…-${nodeId}`): `planned-pr-row`, `internal-status-badge`, `repoint`, `move-up/down`, `sync-merge/rebase`, `base-behind`, `base-conflicts`, `base-conflict-paths`, `base-in-sync`, `base-sync-unavailable`, `pr-link`, `pr-state`, `status-chip`, `start-session`, `worktree`, `parents`, `session`, `child-session`, `child-state`, `branch`, `planned-branch`, `row-toggle`, `row-details`, and others.
- Cypress: `cypress/component/ProjectStackViewAcceptance.cy.tsx` (78 "lists the stack's planned prs and its controls act on that stack"; 97 "new stack creates a stack by address and adds planned prs to it"), plus `PrStackScreenDirectMountAcceptance`, `PlannedPrListStartSessionDirectMountAcceptance`, `PrStackChat*`, `WorkflowChat*`. Page object: `cypress/support/pages/projectStackViewPage.ts`. Stories: `packages/tddy-storybook/src/tddy-prstack-web/PlannedPrPanel.stories.tsx` only.
- Mounts:
  - `packages/tddy-session-web/src/SessionsDrawerDetailPane.tsx:6, 111 (listProjectStacks), 139 (<ProjectStackView …>)`, also imported by `SessionsDrawerScreen.tsx:31`.
  - Route `/sessions/project/:projectId/stack/:stackId` (`tddy-client-web/src/appRoutes.ts:134–153`).
  - Drawer row: `tddy-session-web/src/ProjectStacksRow.tsx` (test ids `sessions-drawer-project-stacks-${projectId}`, `sessions-drawer-project-stack-${stackId}`).
  - `PrStackScreen` is mounted at `tddy-session-web/src/workflowViews.tsx:108`.

#### Proto and RPC
- **`packages/tddy-service/proto/pr_stack.proto`** (464 lines), `service PrStackService` RPCs (16–74): `AddPlannedPr`, `GetPrStatus`, `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch`, `QueryBranch`, `ResolveStackBase`, `LinkStackNode`, `ListProjectStacks`, `GetProjectStack`, `CreateProjectStack`.
  - `StackAddress{project_id, stack_id, daemon_instance_id}` (99–107).
  - `BranchBaseSync` (146–171), including `unavailable`.
  - `BranchWorktree{exists,path,dirty,dirty_paths}` (199–210).
  - `ProjectStackSummary{address,title,node_counts,referencing_session_ids,created_by_session_id,created_at_unix_ms}` (425–436).
  - The plan travels as `stack_plan_json` (a JSON-serialized `Stack`).
- **`packages/tddy-daemon-rpc/src/pr_stack/project_stacks.rs`**:
  - `StackTarget::{Address, Session}` (25–32).
  - `stack_target` (38–57): the address wins.
  - `referencing_sessions_by_stack` (129–153): scans every `changeset.stack_id` on the host.
- **`project_stack_queries.rs:252–289`** `create_project_stack_rpc`: `store.create_titled(project_id, "", title, Stack::default())` — the **creator session is recorded as empty**.
- **`ports.rs`** (880 production lines; tracked by a code issue): RPC bodies, `repoint_planned_pr` at 474, `pull_base_into_branch` at 680.

#### D. Activities and watches
- **`packages/tddy-session-activity/src/session_watches.rs`** (205 lines):
  - Doc 1–13: a watch is a filter plus a log position plus a TTL, persisted per project at `<data dir>/projects/<project_id>/session-watches.json`, keyed by the orchestrator session id.
  - Limits: `MAX_WATCH_TTL_SECS=86_400`, `MAX_LIVE_WATCHES=16`, `MAX_EVENTS_PER_ANSWER=50`.
  - `WatchOwner{project_id,session_id,os_user}`.
  - `SessionWatch{watch_id,owner,filter,created_at_unix_ms,expires_at_unix_ms,position,created_seq}`.
  - `ProjectWatches{watches, ended, inbox_positions: BTreeMap<session,u64>}`.
- **`session_watches/matcher.rs`**:
  - 7–32: `enum WatchKind { Question, Permission, TurnFinished, WaitingForInput, AgentNotification, Ended, Message, TrunkCommit }`.
  - `ALL` (36), `as_str` (48), `parse` (62).
  - `of(record)` (69–96) maps `SessionNotificationSource` to a kind.
  - `WatchFilter{session_ids, started_by_me, kinds}` (101–105).
  - `watch_matches` (118–162): never the owner's own session, except `TrunkCommit`; messages match by recipient or sender.
- **`packages/tddy-daemon-kernel/src/session_notification.rs:22–57`**: `enum SessionNotificationSource { ActivityStatus, AgentToolCall, Presenter, Agent, PendingQuestion, SessionEnded, Message, TrunkCommit }`; `is_a_sessions_own()` = `Message | TrunkCommit`. `SessionNotification` struct at 160+.
- **`project_notification_log.rs`** (499 production lines):
  - `ProjectNotificationRecord{seq, session_id, label, kind, source, text, agent, question, ended, activity_status, message, at_unix_ms, orchestrator_session_id}` (73–100).
  - 439–443, `wants`: `AttentionRequired || source == TrunkCommit`.
  - Storage DTO `project_notification_log/storage/dto.rs:88, 208, 223`.
- **`trunk_watch.rs`**: the template for an activity a daemon task publishes as "the orchestrator's own" (`TrunkWatcher`, `TrunkSource`, `SubscriberLiveness`).
- **Delivery:**
  - `daemon-rpc/exec_tool/watch_events.rs` attaches `watchEvents` to every successful tool answer.
  - `watch_wake.rs:89–95`: `wants(AttentionRequired | ActivityStatus | TrunkCommit)`, then pastes the wake text into the idle orchestrator's main terminal.
  - `project_home_watches.rs:121–152` `create_watch`: only a `trunk_commit`-only self-watch may name the caller itself.
  - `tddy-tools monitor` (`monitor_cli.rs:58`, kinds list) streams via `WaitSessionWatchEvents`.
- **Messaging:**
  - `session_messages.rs` doc 1–23: messages are `Message`-source log records; delivered via an inbox watch or `ReadSessionMessages`, exactly once. `MAX_MESSAGE_BYTES` = 8 KiB.
  - Tools `PostSessionMessage` and `ReadSessionMessages` (`tool-engine/session_message_tools.rs:29–30`).
  - CLI `tddy-tools send --to <id> [--reply-to <mid>] text|-` (`tddy-tools/src/send_cli.rs:1–30`).
- **Tool-side kind list:** `tddy-tool-engine/src/project_home_watch_tools.rs:29–40`, `WATCH_KINDS: [&str; 8]`, "which a test there holds to this list" (`session_watches_unit.rs` `the_kinds_are_named_as_the_tools_name_them`).
- **Wire:** `streams.rs:69` maps `TrunkCommit` to the proto `Unspecified` (the drawer stream skips it); `telegram_session_subscriber.rs:248` declines it.

---

### 3. Grep and glob log (pattern → scope → notable hits)

| Pattern | Scope | Notable hits |
|---|---|---|
| `ProjectStackStore` | `packages/*.rs` | 21 files. Def `tddy-changeset/src/project_stack_store.rs:64,68`. Users: `tool-engine/stack_tools/caller_records.rs`; daemon-rpc `exec_tool/stack_tools.rs`, `stack_git_tools.rs`, `git_work.rs`, `ops.rs`; `pr_stack/project_stack_queries.rs`, `project_stacks.rs`; `session-activity/session_list_enrichment.rs`; `pr-stack/stack_migration.rs`, `stack_ops/mod.rs`; tests in daemon, workflow-recipes, daemon-rpc, changeset |
| `migrate_session_stacks\|ProjectStackStore::new\|create_titled\|.create(` | daemon, daemon-rpc, tool-engine, pr-stack, session-activity, recipes `src` | `last_run_sweep.rs:31`; `exec_tool/stack_tools.rs:143,209`; `stack_git_tools.rs:123`; `project_stack_queries.rs:275`; `project_stacks.rs:193`; `caller_records.rs:126,148`; `stack_migration.rs:47,164` |
| `stack_tools_offered_to\|is_stack_git_tool\|execute_stack_*` | `packages` | `read_only.rs:28–29`; `tddy-tools/server.rs:22–23,354,359`; `svc_list_exec_tools_wiring.rs:14,248`; `exec_tool/project_home.rs:86–88,134,137`; `project_home_remote.rs:37–42,67–68,91,112,257–258` |
| `merge-tree\|merge_base\|rev-list\|--left-right\|ahead_behind\|--count` | `packages` | `session-worktree/base_sync.rs:7–8,185–249`; `worktree-service/base_freshness.rs:90–92,125`; `base_sync_cache.rs`; `subagent-worktree/sync.rs:114–140`, `lineage.rs:20,56`, `diff.rs:53`; `pr-stack/git_ops.rs:118–123`, `repoint.rs:149,168`; `recipes/orchestrate_pr_stack/bridge.rs:135,175`; `review/git_context.rs:62`; `daemon-rpc/git_work/node_ops.rs` (merge-tree) |
| `gh stack\|gh-stack\|gh_stack` | `packages` | none (only an unrelated `core_foundations_shape.rs:154`) |
| `child_recipe` | pr-stack, changeset | `nodes.rs:23–26` "StackNode has no child_recipe" |
| `ProjectStackView` | `*.ts(x)` | `prstack-web/src/{ProjectStackView,index,packageBoundary.test}`; `session-web/src/SessionsDrawerDetailPane.tsx`, `SessionsDrawerScreen.tsx`; cy spec and page object |
| `listProjectStacks\|stack/:stackId` | session-web, web, client-web | `SessionsDrawerDetailPane.tsx:111`; `ProjectStacksRow.tsx:27`; `appRoutes.ts:134–153` |
| `ProjectStack\|PlannedPr\|PrStack` | `tddy-storybook` | `PlannedPrPanel.stories.tsx` only |
| `WATCH_KINDS` | `packages` | `project_home_watch_tools.rs:29,134,236,240` |
| `TrunkCommit\|trunk_commit` | `packages` | 17 files: daemon-kernel; daemon-rpc `project_home_watches.rs:128`, `watch_wake.rs:94`; session-activity `matcher.rs`, `project_notification_log.rs:442`, `dto.rs:88,208,223`, `streams.rs:69`, `trunk_watch.rs`; tool-engine `WATCH_KINDS`; `tddy-tools/monitor_cli.rs:58`; telegram `:248`; github `trunk_feed.rs`; tests |
| `work[ -_]?queue\|workqueue` | repo | only `docs/dev/1-WIP/2026-11-22-workqueue-whole-work-initial-discovery.md` |
| `\bwaves?\b` | `packages/*.rs` | `tddy-build/src/graph.rs:123–125`, `lib.rs:7`, tests |
| `Claimed by` | `docs/dev/todo` | only 3 entries, all unrelated (reshape / file-length gate) |
| Topic grep (`pr[-_ ]stack\|ProjectStackStore\|stack_tools\|merge-tree\|session_watches\|WatchKind\|orchestrat\|repoint\|stack_id…`) | `docs/dev/todo` | ~60 files; triaged in §4F |
| File-name grep (`project_stack_store\|stack_tools\|git_work\|ports.rs\|session_watches\|ProjectStackView\|PlannedPrRow\|PrStackScreen\|base_sync\|watch_wake…`) | `docs/dev/todo` | 22 files; relevant ones read |
| `pr_stack_status\|pr_merge` | `*.yaml *.yml *.toml` | none — no config-driven gating |

---

### 4. Findings

#### A. Stack store

1. **Backend: YAML files, no SQLite.**
   - Path: `<tddy_data_dir>/projects/<project_id>/stacks/<stack_id>.yaml`, where `<tddy_data_dir>` is the sessions base that holds `sessions/` and `projects/`.
   - Every caller builds the store as `ProjectStackStore::new(sessions_base.join("projects"))`.
   - Locking is a per-record in-process mutex, plus a global `FIRST_STACK_WRITE` lock for the session first-write path. Writes are atomic renames.
   - The merge journal sits beside each record: `stacks/<stack_id>.op.json`.
2. **Keyed by project, then stack id.** There is no session key. A session points to a stack via `Changeset.stack_id`.
   - The project id comes from the session's `.session.yaml`.
   - The store's `update` closure receives `&mut Stack` only, so record-level fields (title, creator) cannot be changed through it.
3. **The creating session is recorded** (`StackRecord.created_by_session_id`), with different meanings per path:
   - (a) `update_stack_atomic` first write (`stack.rs:338–365`, the RPC session path and the recipe): the creator is the session's own id, and `changeset.stack_id` is set.
   - (b) `pr_stack_create` tool (`exec_tool/stack_tools.rs:202–219`): the creator is the caller, and `changeset.stack_id` is not set.
   - (c) `CreateProjectStack` RPC (`project_stack_queries.rs:275`): the creator is `""`.
   - (d) Migration: the creator is the legacy session.
   - `refresh_internal_statuses` (`ops.rs:218`) uses `created_by_session_id`'s dir as the "planner dir" for spawn markers.
4. **Data model** (`Stack{version,nodes}` / `StackNode`) — full field list in §2A.
   - Per-node links to sessions: one `session_id` (child) only.
   - Session→stack links: `Changeset.stack_id` (the orchestrator's own stack) and `Changeset.orchestrator_session_id` (child→orchestrator).
   - "Referencing sessions" are computed by scanning changesets (`referencing_sessions_by_stack`).
   - Absent: PR number field, labels/badges, assignee, multi-session association, dependency edges separate from `parents` (`parents` is the only edge kind), waves, schedules, commit lists.
   - `PrInternalStatus` (kind/note/source with override-wins) is the closest existing "badge" concept.
5. **Migrations:**
   - There is one: `tddy_pr_stack::stack_migration::migrate_session_stacks`, run at startup (`last_run_sweep.rs:29–35`).
   - It is idempotent, keyed by `created_by_session_id` (find or create).
   - "No dual read" is a recorded decision (`2026-11-12-project-stack-store.md` § Decisions): readers refuse a changeset still carrying `stack`.
   - Records have no schema/version field; `Stack.version` is a plan version, not a record version.
   - This sweep is the template for a project→session re-key migration.
6. **`tddy-pr-stack` crate:**
   - `stack_ops` writers are generic over `StackHome` (session `Path` or `StoredStack{store,project_id,stack_id}`).
   - Also holds `assess` (`decide_next_action`: a spawn/merge readiness rule), `git_ops`, `pr_insight`, `docs`, `rpc`, `stack_migration`.
   - `orchestrate_pr_stack` (internal status derivation, journal, `execute_stack_merge`/`execute_stack_repoint`) still lives in `tddy-workflow-recipes`, and the daemon reaches into it (`ops.rs:20–22`, `node_ops.rs:18`).

#### B. Agent tools

1. **Plan and read tools (11):** `pr_stack_list`, `pr_stack_create`, `pr_stack_status`, `pr_read`, `pr_search`, `pr_comments`, `pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt`.
2. **Git tools (6):** `pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts`, `pr_spawn_child`. Arguments are in §2B.
3. **Merge, land, propagation:**
   - `pr_merge` only merges and marks `merged`. The description tells the agent to "Then pr_repoint its dependents".
   - The journal records dependents, but nothing repoints them in the daemon path.
   - `pr_repoint` and `pr_set_parents` rebase in the node's worktree, force-push with lease and patch the PR base. They refuse dirty or running nodes.
   - Propagation exists only as the session-addressed recipe `execute_stack_repoint` (`bridge.rs:127`, 120 lines, nesting 8).
   - No tool calls `gh stack`; GitHub is reached through `GithubPrApi` (REST).
4. **Registration and gating are code-only, by role** ("never by recipe", `stack_tools.rs:18–25`):
   - `is_orchestrator_role(session_role)` — `project-home` or `orchestrator` — gets all 17 tools. The project-home catalogue chains `stack_tool_defs()` and `stack_git_tool_defs()`.
   - A session bound to a node of its orchestrator's stack gets `pr_stack_status` and `pr_read` for that stack.
   - Every other session gets nothing.
   - Enforcement happens twice: in the listing (`ListExecTools` `project_home.rs:307–315`, jail `svc_list_exec_tools_wiring.rs`) and at execution (`run_stack_tool`/`run_stack_git_tool` → `permission_denied`).
   - Cross-host calls are forwarded via `daemon_instance_id` (`served_on_target_host`).
   - Read-only orchestrators are allowed (`read_only.rs:28–29`).
   - No YAML or config gating exists.
5. **Existing topological, wave and readiness logic:**
   - `Stack::topo_order` (Kahn), `Stack::display_order`, `effective_base_refs`, `base_ref_for_spawn`.
   - `assess::decide_next_action`; `derive_internal_status` (`ready-to-merge` when all parents are merged).
   - Web `orderStackNodes` / `topoSortStackNodes`.
   - Wave computation exists only in `tddy-build/src/graph.rs::waves` via `kahn_waves`, which is not reusable across crates as-is.
   - No "line" concept exists.

#### C. Web

- **`ProjectStackView`** is mounted by `tddy-session-web`'s `SessionsDrawerDetailPane` on route `/sessions/project/:projectId/stack/:stackId`; the drawer's `ProjectStacksRow` lists stacks via `ListProjectStacks`.
- **Data flow:** `GetProjectStack` once per address → `stack_plan_json` → `parseStackPlan` → `hydrateStackNodes` (joins the session list and `StackChildMetadata`).
- **Live status** is only the per-branch `QueryBranch` poll every 5 s (PR state, worktree dirty, remote, base sync ahead/behind/conflicts, cached server-side by sha). The plan itself is not polled and there is no push stream.
- Mutations replace the plan with the response.
- **`PrStackScreen`** (session-addressed, `usePrStackPlan`) is the older orchestrator-session screen, mounted in `workflowViews.tsx:108`.
- **Coverage:** one Cypress spec for `ProjectStackView` (2 tests) plus a page object; no `ProjectStackView` story (only `PlannedPrPanel.stories.tsx`).

#### D. Activities and watches

1. **Activity model:**
   - `SessionNotification` (bus) → `ProjectNotificationRecord` (per-project log with `seq`) → watches hold a `position`.
   - The source enum lives in `tddy-daemon-kernel`; `WatchKind` lives in `tddy-session-activity` `matcher.rs`.
2. **Adding a watch kind** — the `TrunkCommit` recipe (#983) touches these places:
   - `SessionNotificationSource` variant, plus `is_a_sessions_own` when it is not operator-facing.
   - `WatchKind` variant, `ALL` array (fixed size `[WatchKind; 8]`), `as_str`, `of()`.
   - Matcher special-casing for self-watch.
   - Log `wants`, so the record is kept.
   - Storage DTO `SourceDto` (two `From` arms).
   - `streams.rs` proto mapping (`Unspecified`, so the drawer skips it).
   - Telegram subscriber decline.
   - `watch_wake` `wants`.
   - `CreateSessionWatch` admission in `project_home_watches.rs`.
   - `WATCH_KINDS` in the tool engine (`[&str; 8]`, held equal by a test).
   - `tddy-tools monitor` help text.
   - A publisher (trunk: a daemon task in `svc_trunk_watch_wiring` stepping `TrunkWatcher`).
3. **Delivery to a session:**
   - `watchEvents` attached to every successful tool answer (`watch_events.rs`).
   - Idle wake by pasting into the main terminal (`watch_wake.rs`).
   - `tddy-tools monitor` streaming.
   - Only orchestrators may create non-inbox watches; any session may have an inbox watch.
4. **Messaging:** `PostSessionMessage` / `ReadSessionMessages`, CLI `tddy-tools send --to`, stored once as `Message` records. `pr_resolve_conflicts delegate` uses `send_session_message` or `resume_session`, which is terminal-turn delivery, not `PostSessionMessage`.

#### E. Git helpers (reusable for inbox/outbox, conflict prediction, leak and linearity checks)

- **Ahead/behind:** `tddy_core::base_sync::branch_base_sync`, with `count_divergence` (`rev-list --left-right --count`) and `conflicting_paths` (`merge-tree --write-tree --name-only -z`), plus the sha-keyed cache. `base_freshness` (fetch + ahead/behind); `CutBase`.
- **Merge-tree with an explicit merge-base:** `tddy-subagent-worktree/sync.rs` (async) and `git_work/node_ops.rs::merge_tree_conflicts` (worktree-scoped).
- **Merge-base:** `tddy_pr_stack::git_ops::merge_base`.
- **Rebase:** `rebase_onto`, `rebase_branch_onto_ref`.
- **Push:** `force_push_with_lease`, `push_branch`.
- **Fetch:** `fetch_ref`.
- **Dirty files:** `worktree_is_clean` (tracked only) and `GitEnv::dirty_paths` (porcelain, includes untracked).
- **Commit:** `commit_all_tracked`.
- **Multi-parent integration ref:** `build_integration_ref` (`stack-int/<node>`).
- **Commit listing:** `lineage.rs` (first-parent, no-merges `rev-list`) — closest to a per-item commit list.
- **PR base branch for lineage:** `PrRef.base_branch`, `PrDetail.{base_branch, head_branch}` (`tddy-github/src/pr_api.rs:5–10, 114–129`).
- **GitHub API:** `GithubPrApi { get_open_pr, get_pr_by_head, merge_pr, patch_pr_base, create_pr, disable_auto_merge, close_pr }` and `GithubPrInsightApi { get_pr, list_pr_files, list_check_runs, list_reviews, list_review_comments, list_issue_comments, search_prs, patch_pr_title_body }`.

#### F. Backlog and code-issues touching these nodes (all unclaimed)

**`docs/dev/todo/`:**

| Path | Gist |
|---|---|
| `2026-07-26-pr-stack-status-polling-and-stack-hygiene.md` | Polling volume is linear (batch per stack / ETag). Dangling `session_id` links are never scrubbed. `origin/<branch>` freshness depends on the last fetch, so a repoint can wrongly drop an edge. A repoint doesn't show which edges it drops. `repoint_planned_pr_node` has 3 silent-failure paths (empty `expected_sha`, invented `merge_base`, force-push failure only warned). |
| `2026-07-30-pr-stack-full-control-follow-ups.md` | No web surface for update/delete/set-parents/adopt. `pr_delete_planned` leaves branch/worktree/session. `child_recipe` is inert. `topo_order` ignores dangling parents. "update_stack_atomic takes no lock" (now superseded by the store's per-record lock). `pr_search` pagination. No GraphQL resolved-thread state. No testable transport seam for `RealGithubPrApi`. |
| `2026-08-30-pr-stack-querybranch-s-local-only-legs.md` | The worktree and remote legs answer only for the asked host; `resolveRepointTarget` is wrong cross-host; sandboxed cursor-cli does no node link. |
| `2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions.md` | Only one base session can seed; multi-select ordering would move `parents` (`pr_set_parents`), not rows. |
| `2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md` | A path-prefix check should compare the git common dir. |
| `2026-08-13-pr-stack-a-managed-claude-cli-cursor-cli-session-may-record-a-goal-id.md` | Recipe goal-id-vs-state spelling. |
| `2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md` | The documented progress file is never written; sync is via `sync_stack_node_from_child`. |
| `2026-11-14-retire-stack-child-spawn-handler.md` | `StackChildSpawnHandler` and the `spawn-child` plumbing are unreachable since #887; deferred. |
| `2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md` | A node session's own host doesn't list its 2 reads when the stack is held elsewhere (classification reads the local store). Directly relevant to per-session keying. |
| `2026-11-16-session-watches-are-not-shown-on-the-web.md` | No RPC or view for watches. |
| `2026-11-21-base-freshness-tools-are-not-forwarded-to-a-peer-host.md` | `CheckBaseFreshness`/`RefreshHome` are local only. |
| `2026-11-21-trunk-watch-reads-github-com-only.md` | `owner_repo_from_remote_url` only recognises github.com; `RestTrunkFeed` hardcodes api.github.com. |
| `2026-11-20-session-liveness-oversized-files-after-the-stack.md` | Lists `daemon-rpc/src/pr_stack/ports.rs` 878→880. |
| `2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md` | Historical (the crate now has `tests/`). |
| `2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md` | `stack_child_spawn_tests.rs:395` flake. |
| `2026-11-22-coder-target-back-channel-and-activity-cursor.md` | Orchestrator↔target-session delivery is still unverified live. |

**Code-issue records (all unclaimed):**

| Record | Metric |
|---|---|
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md` | 880 production lines vs a 500 budget, accepted as standing debt 2026-11-14 (restructure engine refused); candidate seams A–… listed |
| `packages/tddy-pr-stack/docs/code-issues/complexity-repoint-repoint-planned-pr-node.md` | 78–79 lines, 6 params |
| `packages/tddy-pr-stack/docs/code-issues/complexity-pull-base-pull-base-into-node-branch.md` | 157 lines, 7 params, nesting 4 |
| `packages/tddy-workflow-recipes/docs/code-issues/complexity-bridge-execute-stack-repoint.md` | 120 lines, nesting 8 — the propagation prototype |
| `packages/tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md` | Its `**Claimed by:**` reads "— unclaimed"; partially fixed by #496 |
| `packages/tddy-session-activity/docs/code-issues/complexity-service-stream-acp-replay.md` | Not on the stack path |
| `packages/tddy-tools/docs/code-issues/oversized-file-server.md` | `server.rs` dispatches stack tools |
| `packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-list-exec-tools.md` | Tool listing |

The `prstack-web` README mentions a code-issue for `PrStackScreen.tsx` being over 500 lines, but no `code-issues/` directory exists there, and the file is now 395 lines.

**Files near budget** (production lines up to the first `#[cfg(test)]`, against a 500 budget):

| File | Production lines |
|---|---|
| `tddy-changeset/src/changeset/stack.rs` | 463 |
| `tddy-tool-engine/src/stack_tools.rs` | 463 |
| `tddy-pr-stack/src/git_ops.rs` | 468 |
| `tddy-daemon-rpc/src/exec_tool/project_home.rs` | 468 |
| `tddy-daemon-rpc/src/exec_tool/stack_git_tools.rs` | 432 (no test module) |
| `tddy-daemon-rpc/src/exec_tool/stack_git_tools/git_work.rs` | 430 |
| `tddy-session-activity/src/project_notification_log.rs` | 499 — any new source arm likely pushes it over |

**Existing test suites to extend or mirror:**

| Suite | Tests |
|---|---|
| `tddy-changeset/tests/project_stack_store_acceptance.rs` | 6 |
| `tddy-pr-stack/tests/stack_migration_sweep_acceptance.rs` | 4 |
| `tddy-daemon/tests/startup_stack_migration_acceptance.rs` | 1 |
| `tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs` | 10 |
| `tddy-daemon-rpc/tests/stack_git_tools_acceptance.rs` | 7 |
| `tddy-daemon-rpc/tests/project_stacks_rpc_acceptance.rs` | 7 |
| `tddy-tool-engine/tests/stack_tools_catalogue.rs` | 1 |
| `tddy-session-activity/tests/session_watches_unit.rs` | 14 |
| `tddy-session-activity/tests/trunk_watch_acceptance.rs` | 6 |
| `prstack-web` `ProjectStackViewAcceptance.cy.tsx` | 2 |

Fixtures: `tddy-changeset/src/stack_fixtures.rs` (`a_stack_host`, `stack_id_of`, `stack_on_disk`, `seed_session_stack`, `A_PROJECT`).

## Exploration 4 — node follow-up: does listing resolve the recipe?

- `rg -n 'unknown_workflow_recipe_error' packages -g '*.rs' -g '!**/tests/**'` → only
  `tddy-agent-skills/src/feature_start_slash.rs:24,104` and `tddy-workflow-recipes/src/recipe_resolve.rs:19,118`.
- `rg -l 'recipe_resolve::|resolve_workflow_recipe' packages -g '*.rs' -g '!**/tests/**'` →
  `tddy-agent-launch/src/{placement.rs, svc_resume_sandboxed_cli_harness_session.rs,
  stack_seed_validation.rs, cli_agent_steps/resumed_wiring.rs}`, `tddy-coder/src/{recipe.rs, run.rs, lib.rs}`,
  `tddy-e2e/src/lib.rs`, `tddy-workflow-recipes`.
- **Finding:** no listing path (`tddy-session-activity`, `tddy-session-lifecycle`) resolves the recipe;
  `session_list_enrichment::stack_plan_json_for_changeset` keys on `stack_id`. A legacy recipe name
  breaks resume and placement only — which is what the resume-refusal test pins.

## Exploration 5 — pass 2: where resume resolves a recorded recipe

- `rg -n 'resolve_workflow_recipe|workflow_recipe_and_manifest' packages/tddy-agent-launch/src/{placement.rs,cli_agent_steps/resumed_wiring.rs,svc_resume_sandboxed_cli_harness_session.rs}`
  → `placement.rs:201`, `svc_resume_sandboxed_cli_harness_session.rs:68` (`recorded_managed_recipe`,
  maps the error to `invalid_argument`), `resumed_wiring.rs:83`.
- **Finding:** once the names leave `recipe_resolve.rs`, resume refuses through
  `unknown_workflow_recipe_error`, which names the requested recipe; the unit test pins that through
  `recorded_managed_recipe`.
- Cypress: a `should("not.exist")` assertion right after mount passes vacuously before the form renders;
  the create-session spec waits for the recipe control first.

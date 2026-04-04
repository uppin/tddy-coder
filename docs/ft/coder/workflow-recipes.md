# Workflow recipes (pluggable workflows)

**Product area:** Coder  
**Updated:** 2026-04-04

## Summary

Workflow behavior is defined by **recipes** in the **`tddy-workflow-recipes`** crate. **`tddy-core`** implements a recipe-agnostic engine (`WorkflowRecipe`, `WorkflowEngine`, graph execution, `CodingBackend`). Goals, states, transitions, hooks, backend hints, and permissions are **recipe-provided strings and metadata**, not a fixed enum in core.

The shipped recipes are **`TddRecipe`** (default), **`TddSmallRecipe`**, **`BugfixRecipe`**, **`FreePromptingRecipe`**, and **`GrillMeRecipe`**. Recipe selection uses a single resolution path in **`tddy-workflow-recipes::recipe_resolve`**: `workflow_recipe_and_manifest_from_cli_name` and `resolve_workflow_recipe_from_cli_name` return the active `WorkflowRecipe` (and, where needed, the paired **`SessionArtifactManifest`** on the same concrete type).

## Selecting a recipe

| Surface | Mechanism |
|---------|-----------|
| **tddy-coder** | `--recipe tdd`, `--recipe tdd-small`, `--recipe bugfix`, `--recipe free-prompting`, or `--recipe grill-me`; optional YAML `recipe:` (CLI overrides). |
| **changeset.yaml** | Optional `recipe:` records the workflow for resume and session lists; empty or absent values behave like **`tdd`**. Initial session creation (presenter bootstrap and matching CLI paths) persists **`recipe`** on the written **`changeset.yaml`** so resume and tooling read the same recipe name as **`StartSession`**. |
| **tddy-daemon** | Spawns **`tddy-coder`** with `--recipe` when set; **`ConnectionService` `StartSessionRequest`** and **`TddyRemote` `StartSession`** carry a **`recipe`** string. |
| **tddy-web** | **ConnectionScreen** exposes a **Workflow recipe** control per **Start New Session**; the value is sent on **`StartSession`**. |

Allowed names are **`tdd`**, **`tdd-small`**, **`bugfix`**, **`free-prompting`**, and **`grill-me`** (aligned with **`WorkflowRecipe::name()`**). Invalid names fail on the CLI with a clear error; daemon streams report failure via **`WorkflowComplete`** with a descriptive message that lists supported names.

## TddRecipe

- **Start goal:** **`plan`**
- **Pipeline:** plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs (full TDD graph).
- **Primary session document:** **`prd`** → **`PRD.md`** under the session artifact layout (see **`SessionArtifactManifest`**).
- **Session document approval:** Hook-driven **`ElicitationEvent::DocumentApproval`** after the plan task when **`WorkflowRecipe::uses_primary_session_document`** is **`true`** and the primary document is readable; the presenter and plain CLI use the same recipe-driven gate.

## TddSmallRecipe

- **CLI / recipe name:** **`tdd-small`** (**`WorkflowRecipe::name()`**).
- **Start goal:** **`plan`**.
- **Pipeline:** **`plan` → `red` → `green` → `post-green-review` → `refactor` → `update-docs` → `end`**. There are no separate graph nodes for **`acceptance-tests`**, **`demo`**, **`evaluate-changes`**, or **`validate`**; the **`red`** step uses merged red/evaluate-style prompt text, and **`post-green-review`** is a single structured submit that covers evaluate- and validate-style reporting (see **`tddy-tools get-schema post-green-review`**).
- **Primary session document:** **`prd`** → **`PRD.md`** (same manifest pattern as full TDD); **`TddSmallRecipe::uses_primary_session_document`** is **`true`** so the plan approval gate applies after **`plan`**.
- **Structured submit:** Goals follow the same JSON Schema registry as full TDD where applicable; **`post-green-review`** has a dedicated schema and parser (**`parse_post_green_review_response`**) in **`tddy-workflow-recipes`**.

## BugfixRecipe

- **Start goal:** **`analyze`**
- **Pipeline:** **`analyze` → `reproduce` → `end`**. The **`analyze`** goal uses structured **`tddy-tools submit`** output (JSON Schema goal **`analyze`**) to record **`branch_suggestion`**, **`worktree_suggestion`**, optional **`name`**, and optional **`summary`** on **`changeset.yaml`** (with **`summary`** also available for the **`reproduce`** prompt via **`changeset.artifacts["analyze_summary"]`**). The **`reproduce`** goal does not require **`tddy-tools submit`** by default (**`goal_requires_tddy_tools_submit`** is **`false`** for **`reproduce`**).
- **Primary session document:** The manifest registers **`fix_plan` → `fix-plan.md`** for tooling and prompts; **`BugfixRecipe::uses_primary_session_document`** is **`false`**, so the hook-driven PRD-style primary-document approval gate does not run for this recipe.
- **Product alignment:** The workflow combines triage and branch/worktree naming (**`analyze`**) with **reproduce-then-fix** discipline (**`reproduce`**) and focused test repair (small verification loops).

## FreePromptingRecipe

- **CLI / recipe name:** **`free-prompting`** (**`WorkflowRecipe::name()`**).
- **Start goal:** **`prompting`**; **initial workflow state string:** **`Prompting`**.
- **Pipeline:** A single graph node (**`prompting`**) implemented with **`BackendInvokeTask`**: each turn invokes the active **`CodingBackend`** (stub, Cursor, Claude, …). There is no separate **`EndTask`** edge; after a successful turn the engine pauses for the next user line (**`FlowRunner`** treats **`Continue`** with no successor task like **`WaitingForInput`** so the session stays on **`prompting`**). No multi-goal TDD pipeline unless the recipe is extended later.
- **Structured submit:** **`WorkflowRecipe::goal_requires_tddy_tools_submit`** defaults to **`true`** for TDD-style goals; **`FreePromptingRecipe`** returns **`false`** for **`prompting`** so a turn can complete from normal agent output without **`tddy-tools submit`** (open-ended chat with backends that do not relay the tool).
- **Activity pane:** **`FreePromptingWorkflowHooks::agent_output_sink`** forwards streaming assistant text to **`WorkflowEvent::AgentOutput`**, same pattern as **`TddWorkflowHooks`**, so the TUI activity log shows assistant output during the run.
- **Primary session document:** None in the manifest sense used for PRD-style approval; **`FreePromptingRecipe::uses_primary_session_document`** is **`false`**, so the primary-document approval gate for plan/fix-plan style review does not apply for this recipe.
- **Policy helpers:** **`tddy_workflow_recipes::approval_policy`** exposes **`supported_workflow_recipe_cli_names`** and **`recipe_should_skip_session_document_approval`** for tests and tooling that document which CLI names participate in resolver errors and which recipes skip session-document approval in policy tables.

## GrillMeRecipe (Updated: 2026-04-05)

- **CLI / recipe name:** **`grill-me`** (**`WorkflowRecipe::name()`**).
- **Goals:** **`grill`** (clarify) then **`create-plan`** (write brief). **Start goal:** **`grill`**; **initial workflow state string:** **`Grill`**.
- **Pipeline:** **`grill` → `create-plan` → `end`**. **Grill** uses **`BackendInvokeTask`**. The **Grill** system prompt instructs the agent to submit clarification through **`tddy-tools ask`** (JSON payload; **`TDDY_SOCKET`** relay → presenter / TUI). Backends that emit AskQuestion-style **stream** events can still populate **`InvokeResponse.questions`** → **`WaitForInput`** (multi-turn); when a turn returns **no questions**, the task **`Continue`s** to **`create-plan`**. **Create plan** invokes the backend with a system prompt that requires **`artifacts/grill-me-brief.md`**; hooks inject **Grill** vs **Create plan** system prompts and forward **`answers` → `prompt`**; for **Create plan**, the user message is assembled from **`feature_input`**, prior **`output`**, and **`answers`** so Q&A is visible to the model.
- **Structured submit:** **`false`** for **`grill`** and **`create-plan`** (same pattern as **free-prompting**: turns complete without **`tddy-tools submit`**).
- **Session directory:** **`goal_requires_session_dir`** is **`true`** for **`grill`** and **`create-plan`**.
- **Artifacts:** **`grill_brief` → `grill-me-brief.md`** in **`SessionArtifactManifest`** (written in **Create plan** under the session **`artifacts/`** tree; not used for PRD-style approval in v1).
- **Repo persistence:** For the **working copy**, persist the same brief content under a repo path per **[AGENTS.md](../../../AGENTS.md)** (**Documentation Hierarchy** → **`plans/`**): use a path specified in **`docs/ft/`** for the feature when present; otherwise **`plans/<SOME-PLAN-NAME>.md`** at the repository root (descriptive basename, e.g. **`<feature-slug>-grill-me-brief.md`**).
- **Primary session document:** **`GrillMeRecipe::uses_primary_session_document`** is **`false`**; policy skips session-document approval alongside **free-prompting**.

## Key types

| Concept | Role |
|---------|------|
| **`GoalId`**, **`WorkflowState`** | String-backed identifiers (serde-transparent); core does not match on fixed goal names. |
| **`WorkflowRecipe`** | Trait: graph, hooks, state machine helpers, permissions, artifacts, backend hints (`GoalHints` / `PermissionHint`). |
| **`TddRecipe`** | Full TDD workflow graph, `TddWorkflowHooks`, parsers, plan task wiring. |
| **`TddSmallRecipe`** | Shortened TDD graph (`plan` → merged **`red`** → **`green`** → **`post-green-review`** → **`refactor`** → **`update-docs`**), `TddSmallWorkflowHooks`, merged red and post-green prompts. |
| **`BugfixRecipe`** | Bugfix workflow graph (**`analyze` → `reproduce` → `end`**) hooks, and artifact manifest for **`analyze`**, **`reproduce`**, and fix-plan. |
| **`FreePromptingRecipe`** | Minimal graph and hooks for the **Prompting** loop without TDD gates. |
| **`GrillMeRecipe`** | Two goals (**`grill`** → **`create-plan`**); session **`artifacts/grill-me-brief.md`**; repo copy per **AGENTS.md** / **`plans/`** default. |
| **`approval_policy`** | Supported CLI name list and skip rules aligned with **`recipe_resolve`** and acceptance tests. |
| **`recipe_resolve`** | **`workflow_recipe_and_manifest_from_cli_name`**, **`resolve_workflow_recipe_from_cli_name`**, **`unknown_workflow_recipe_error`**, **`WorkflowRecipeAndManifest`**. |

## CLI and services

- **`--goal`** accepted values come from the active recipe’s declared goals (no hard-coded enum in the CLI).
- **gRPC / daemon** use string goals and states; **`DaemonService`** loads the selected recipe via **`workflow_recipe_and_manifest_from_cli_name`** when handling **`StartSession`** (including **`free-prompting`** and **`grill-me`** when requested).

## Packages

| Package | Responsibility |
|---------|----------------|
| **`tddy-core`** | `WorkflowRecipe` trait, `WorkflowEngine` parameterized by `Arc<dyn WorkflowRecipe>`, session storage, backends, presenter integration. |
| **`tddy-workflow-recipes`** | Concrete recipes, **`recipe_resolve`**, **`approval_policy`**, hooks, parsers, and backend hints per recipe. |
| **`tddy-coder`**, **`tddy-service`**, **`tddy-daemon`**, **`tddy-demo`** | Resolve the active recipe from CLI, config, changeset, or RPC; default **`tdd`** when unspecified. |

## Structured output contracts

JSON Schemas for workflow goals (`plan`, `red`, `green`, etc.) live in **`tddy-workflow-recipes`**; **`tddy-tools`** embeds them for `get-schema`, `list-schemas`, and `submit` validation. See [Workflow JSON Schemas](workflow-json-schemas.md) for the registry (`goals.json`), CLI behavior, and links to package-level technical notes.

## Session artifacts and primary planning documents

**Goal IDs** (e.g. `"plan"`, `"analyze"`, `"reproduce"`, `"prompting"`, `"grill"`, `"create-plan"`) stay stable as wire/API identifiers. (**`grill-me`** is the **recipe** CLI name, not a goal id.) **Filenames and on-disk layout** for the primary planning document and related artifacts are defined by each recipe’s manifest (**`SessionArtifactManifest`**, `default_artifacts` / `known_artifacts`), not by fixed defaults inside **`tddy-core`**.

- **`tddy-core`** exposes **`WorkflowRecipe::uses_primary_session_document`** and **`read_primary_session_document_utf8`** for approval gates, plain CLI, and daemon flows.
- **`tddy-workflow`** provides **`artifact_paths`** helpers (`session_dir/artifacts/`, legacy `sessions/<uuid>/` layouts, resolution order).
- **TDD** uses **`prd` → `PRD.md`** in its manifest; **Bugfix** registers **`fix_plan` / `fix-plan.md`** with **`uses_primary_session_document`** **`false`** (no automatic document-approval gate on that path); **Free prompting** does not define a primary planning basename for that approval path; **Grill me** registers **`grill_brief` → `grill-me-brief.md`** without using it for the primary-document approval gate in v1; long-lived copy in the repo follows **AGENTS.md** (**`plans/`** or a **`docs/ft/`**-specified path).

Custom recipes **declare** artifact keys in manifest; there is no silent core fallback string for the primary planning basename.

## Developer reference (shipped recipes)

This section records how the shipped recipes map to the same product philosophy as the repo’s Cursor-oriented commands.

### TDD (`tdd`)

- **Default** when **`--recipe`** is omitted or **`changeset.yaml`** has no **`recipe`** field (backward compatible).
- **Start goal:** **`plan`** — greenfield planning, PRD/TODO-style artifacts, full graph (plan → acceptance-tests → red → green → …).
- **Spirit:** Aligns with a typical feature-development workflow (plan first, then tests and implementation).

### TDD-small (`tdd-small`)

- **Start goal:** **`plan`** — same planning and PRD-style artifacts as full TDD.
- **Pipeline:** **`plan` → `red` → `green` → `post-green-review` → `refactor` → `update-docs`** — a linear graph without standalone acceptance-tests, demo, evaluate, or validate tasks; **`post-green-review`** carries merged reporting concerns in one **`tddy-tools submit`** payload.
- **Spirit:** Smaller session surface for teams that want TDD discipline without the full optional branches (demo routing, separate evaluate/validate invocations).

### Bugfix (`bugfix`)

- **Start goal:** **`analyze`** — derive branch name, worktree directory name, optional changeset title, and optional short triage summary from the bug report (structured **`tddy-tools submit`** for **`analyze`**).
- **Pipeline:** **`analyze` → `reproduce` → `end`**. **`reproduce`** confirms or creates a failing test / deterministic reproduction before deeper fix work.
- **Artifacts:** **fix-plan** content (e.g. **`fix-plan.md`** under the session artifact layout) and **`changeset.yaml`** fields populated from **`analyze`** submit.
- **Spirit:** Combines naming discipline with **`.cursor/commands/reproduce.md`** (reproduction discipline) and **`.cursor/commands/fix-tests.md`** (focused diagnosis and fix, small verification loops).
- **Session document approval:** **`uses_primary_session_document`** is **`false`** for **bugfix**, so the automatic PRD-style document-approval step does not run; **`fix-plan.md`** remains a manifest artifact for context and tooling.

### Free prompting (`free-prompting`)

- **Start goal:** **`prompting`** — open-ended agent turns without the TDD multi-goal pipeline.
- **Artifacts:** No PRD-style primary session document on the approval path; session document approval for that mechanism is skipped via **`uses_primary_session_document`** **`false`**.
- **Spirit:** Unconstrained iteration when the product does not require plan/PRD or fix-plan gates.

### Grill me (`grill-me`)

- **Start goal:** **`grill`** — clarify using **`tddy-tools ask`** so questions reach the TUI via the socket relay; stream-based **`InvokeResponse.questions`** is an additional path when the backend emits those events. **Next goal:** **`create-plan`** — consumes Q&A and original input, writes **`artifacts/grill-me-brief.md`** (problem, Q&A, analysis, preliminary implementation plan).
- **Artifacts:** **`grill-me-brief.md`** registered on the manifest for tooling/context; v1 does not use the PRD-style primary-document approval gate.
- **Repo copy:** Persist the brief in the target repo per **[AGENTS.md](../../../AGENTS.md)**; default **`plans/<SOME-PLAN-NAME>.md`** when no feature doc path applies.
- **Spirit:** Structured discovery before implementation (interview-style elicitation, then a single planning artifact checked in for the team).

### Tests

- **Rust:** **`./test`** from the repo root is the primary gate (builds required binaries including **`tddy-acp-stub`**, then runs **`cargo test`** with **`--test-threads=1`**).
- **Web:** Cypress component/e2e for **`tddy-web`** are **not** included in **`./test`**; run from the repo via **`bun run cypress:component`** / **`cypress:e2e`** under **`packages/tddy-web`** (or root scripts that filter **`tddy-web`**). Ensure workspace install so **`tddy-livekit-web`** resolves (Vite aliases **`tddy-livekit-web`** to package source for dev/Cypress).

## Session context and conditional transitions

Workflow transitions read **boolean conditions** from the engine `Context` (declarative `goal_conditions` on `WorkflowTransition`). The TDD recipe supplies keys such as **`run_optional_step_x`** so the full graph branches after green without presenter-specific branching for demo vs evaluate.

**Session storage:** **`tddy-tools set-session-context`** merges JSON into **`.workflow/<session-id>.session.json`** under the session directory (`TDDY_SESSION_DIR`, `TDDY_WORKFLOW_SESSION_ID`). Values sync into the workflow engine context via **`Context::merge_json_object_sync`** before transition evaluation.

**Recipe hooks:** Green-completion hooks in the TDD recipe instruct the agent to call **`tddy-tools ask`**, then **`tddy-tools set-session-context`** with `{"run_optional_step_x":true}` or `{"run_optional_step_x":false}` so the next transition predicate matches.

**Authoring:** Declare **`goal_conditions`** on transitions in workflow JSON; use **`merge_json_object_sync`**-compatible keys in session documents. See [Workflow JSON Schemas](workflow-json-schemas.md) for the goals registry and schema layout.

## Related

- [Coder overview](1-OVERVIEW.md) — capabilities and integration points  
- [Workflow JSON Schemas](workflow-json-schemas.md) — schema ownership, `tddy-tools` CLI, `goals.json`  
- [Planning step](planning-step.md), [Implementation step](implementation-step.md) — goal-level behavior  
- [gRPC remote control](grpc-remote-control.md) — string goals/states in RPC  

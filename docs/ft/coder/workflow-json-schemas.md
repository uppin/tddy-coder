# Workflow JSON Schemas (structured agent output)

**Product area:** Coder  
**Updated:** 2026-04-06

## Summary

Structured outputs for workflow goals (including **`analyze`** for the bugfix recipe, **`branch-review`** for the **review** recipe, plus plan, red, green, acceptance-tests, **post-green-review**, evaluate-changes, validate, refactor, update-docs, demo, and **`changeset-workflow`** for changeset-scoped workflow JSON) are defined as **JSON Schema** artifacts under **`generated/{recipe}/`** in **`tddy-workflow-recipes`**, registered in **`goals.json`**. The **`tddy-tools`** binary embeds those schemas, validates `submit` payloads, exposes **`get-schema <goal>`**, and lists registered goals via **`list-schemas`**. Each registry entry lists the CLI goal name, schema filename, and Protocol Buffer filename so registry drift is testable.

## Source layout

| Location | Role |
|----------|------|
| `packages/tddy-workflow-recipes/goals.json` | Registry: `name`, `schema`, `proto` per workflow goal |
| `packages/tddy-workflow-recipes/generated/{recipe}/` | JSON Schema files per goal (e.g. `generated/tdd/post-green-review.schema.json`) plus `common/` refs |
| `packages/tddy-workflow-recipes/proto/` | Protocol Buffer messages documenting the same contracts at the IDL layer |
| `packages/tddy-workflow-recipes/generated/` | `schema-manifest.json`, `proto_basenames.rs`, and embedded schema tree consumed by **`tddy-tools`** |

## tddy-tools behavior

- **Embedding**: Goal schemas and `common/` resources ship inside the binary from `generated/`.
- **`get-schema <goal>`**: Prints the JSON Schema for that goal (optional `-o` writes the goal file and common schemas).
- **`list-schemas`**: Prints JSON `{"goals":["plan",...]}` in stable sorted order for automation.
- **`submit --goal <name>`**: Validates stdin JSON against the goal schema before optional relay to `tddy-coder`; validation tips reference `get-schema` and `list-schemas`. For **`branch-review`**, after validation succeeds, **`review.md`** is written under **`TDDY_SESSION_DIR`** when that environment variable is set.
- **Input limit**: Submit and ask read at most 16 MiB from stdin or `--data` to bound memory use.

## Relationship to recipes

Workflow **behavior** (graphs, hooks, parsers) lives in **`tddy-workflow-recipes`** recipes. **Schema contracts** for agent-facing JSON are shared through `goals.json` and the paths above; see [Workflow recipes](workflow-recipes.md) for pluggable workflow architecture.

## Session context CLI (`set-session-context`)

**`tddy-tools set-session-context`** merges a JSON object into the workflow session file (`.workflow/<id>.session.json`). Environment: **`TDDY_SESSION_DIR`** (session root), **`TDDY_WORKFLOW_SESSION_ID`** (session id). The merge aligns with the workflow engine: values feed **`Context::merge_json_object_sync`** so **`goal_conditions`** on transitions evaluate against the same key/value map.

This command is **not** listed in **`goals.json`**; it is a session utility, not a JSON-schema-backed planning goal. See **`packages/tddy-tools/docs/json-schema.md`** for the CLI table.

## Changeset workflow (`persist-changeset-workflow`)

**`tddy-tools persist-changeset-workflow`** takes **`--session-dir`** (directory containing **`changeset.yaml`**) and **`--data`** (JSON object). Payloads validate against the **`changeset-workflow`** schema (**`$id`**: **`urn:tddy:tool/changeset-workflow`**), then merge into **`changeset.yaml`** under **`workflow`** with an atomic replace. This goal is listed in **`goals.json`** alongside workflow goals so **`get-schema changeset-workflow`** and **`list-schemas`** include it. It complements **`set-session-context`**: the latter updates ephemeral session JSON; **`persist-changeset-workflow`** updates the durable changeset manifest.

The **`changeset-workflow`** schema includes post-green routing fields (**`run_optional_step_x`**, **`demo_options`**, optional **`tool_schema_id`**) and branch/worktree intent fields:

- **`branch_worktree_intent`**: string enum **`new_branch_from_base`** | **`work_on_selected_branch`** (optional).
- **`selected_integration_base_ref`**: remote-tracking ref used when creating a new branch from a base (for example **`origin/main`**).
- **`new_branch_name`**: branch name for **`new_branch_from_base`**.
- **`selected_branch_to_work_on`**: existing local branch name for **`work_on_selected_branch`**.

**`additionalProperties`** is **`false`** on the payload object: unknown keys fail validation.

**`tddy_core::changeset::merge_persisted_workflow_into_context`** copies **`run_optional_step_x`**, **`demo_options`**, and intent-related keys (**`branch_worktree_intent`**, **`selected_integration_base_ref`**, **`new_branch_name`**, **`selected_branch_to_work_on`**) into the engine **`Context`** when present so hooks and resume logic read the same values as **`changeset.yaml`**.

**`tddy_core::worktree`** uses persisted workflow intent in **`setup_worktree_for_session_with_integration_base`** and **`setup_worktree_for_session_with_optional_chain_base`**: **`new_branch_from_base`** creates a new branch and worktree from the integration base; **`work_on_selected_branch`** attaches a worktree at the suggested directory name to an existing branch (including a detach-and-switch path when Git reports the branch is already checked out elsewhere).

## Related

- [Workflow recipes](workflow-recipes.md) — `TddRecipe`, goals as strings, engine integration  
- `docs/dev/1-WIP/workflow-schema-pipeline.md` — build pipeline and editing workflow  
- `packages/tddy-tools/docs/json-schema.md` — CLI and library technical details  
- `packages/tddy-workflow-recipes/docs/workflow-schemas.md` — crate-owned schema and proto layout  

# Workflow JSON Schemas (structured agent output)

**Product area:** Coder  
**Updated:** 2026-04-04

## Summary

Structured outputs for workflow goals (including **`analyze`** for the bugfix recipe, plus plan, red, green, acceptance-tests, evaluate-changes, validate, refactor, update-docs, demo) are defined as **JSON Schema** artifacts owned by **`tddy-workflow-recipes`**. The **`tddy-tools`** binary embeds those schemas, validates `submit` payloads, exposes **`get-schema <goal>`**, and lists registered goals via **`list-schemas`**. A single **`goals.json`** registry lists each CLI goal name, schema filename, and Protocol Buffer filename so registry drift is testable.

## Source layout

| Location | Role |
|----------|------|
| `packages/tddy-workflow-recipes/goals.json` | Registry: `name`, `schema`, `proto` per workflow goal |
| `packages/tddy-workflow-recipes/schemas/` | Authoritative JSON Schema files (including `common/`) |
| `packages/tddy-workflow-recipes/proto/` | Protocol Buffer messages documenting the same contracts at the IDL layer |
| `packages/tddy-workflow-recipes/generated/` | Build output: copied schemas, `schema-manifest.json`, generated Rust snippets for proto basenames |

## tddy-tools behavior

- **Embedding**: Goal schemas and `common/` resources ship inside the binary from `generated/`.
- **`get-schema <goal>`**: Prints the JSON Schema for that goal (optional `-o` writes the goal file and common schemas).
- **`list-schemas`**: Prints JSON `{"goals":["plan",...]}` in stable sorted order for automation.
- **`submit --goal <name>`**: Validates stdin JSON against the goal schema before optional relay to `tddy-coder`; validation tips reference `get-schema` and `list-schemas`.
- **Input limit**: Submit and ask read at most 16 MiB from stdin or `--data` to bound memory use.

## Relationship to recipes

Workflow **behavior** (graphs, hooks, parsers) lives in **`tddy-workflow-recipes`** recipes. **Schema contracts** for agent-facing JSON are shared through `goals.json` and the paths above; see [Workflow recipes](workflow-recipes.md) for pluggable workflow architecture.

## Session context CLI (`set-session-context`)

**`tddy-tools set-session-context`** merges a JSON object into the workflow session file (`.workflow/<id>.session.json`). Environment: **`TDDY_SESSION_DIR`** (session root), **`TDDY_WORKFLOW_SESSION_ID`** (session id). The merge aligns with the workflow engine: values feed **`Context::merge_json_object_sync`** so **`goal_conditions`** on transitions evaluate against the same key/value map.

This command is **not** listed in **`goals.json`**; it is a session utility, not a JSON-schema-backed planning goal. See **`packages/tddy-tools/docs/json-schema.md`** for the CLI table.

## Related

- [Workflow recipes](workflow-recipes.md) — `TddRecipe`, goals as strings, engine integration  
- `docs/dev/1-WIP/workflow-schema-pipeline.md` — build pipeline and editing workflow  
- `packages/tddy-tools/docs/json-schema.md` — CLI and library technical details  
- `packages/tddy-workflow-recipes/docs/workflow-schemas.md` — crate-owned schema and proto layout  

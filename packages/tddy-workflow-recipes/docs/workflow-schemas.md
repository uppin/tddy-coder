# Workflow schemas and protos (`tddy-workflow-recipes`)

## Purpose

This crate owns the **JSON Schema** contract tree under **`schemas/`**, the **Protocol Buffer** sources under **`proto/`**, and the **`goals.json`** registry that ties each CLI goal to a schema file and a proto basename.

## Registry (`goals.json`)

Each entry contains:

- **`name`** — CLI goal string (e.g. `evaluate-changes`, `validate`).
- **`schema`** — File under `schemas/` (e.g. `evaluate.schema.json`).
- **`proto`** — File under `proto/` (e.g. `evaluate_changes.proto`).

The build script validates that every referenced schema and proto file exists, copies schemas into **`generated/`**, writes **`generated/schema-manifest.json`** (includes `proto` per goal), and emits **`generated/proto_basenames.rs`** for `schema_pipeline::expected_proto_basenames()`.

## Module `schema_pipeline`

Exposes **`proto_root()`**, **`expected_proto_basenames()`**, and **`generated_manifest_path()`** for tests and diagnostics. The name reflects the future **proto → JSON Schema** codegen path; the JSON Schema files under **`schemas/`** remain the validation source until that toolchain is integrated.

## Tests

- **`tests/goals_contract.rs`** — `goals.json` names match `generated/schema-manifest.json`; count matches proto basename list.
- **`tests/proto_goal_files.rs`** — Expected proto files exist; generated manifest exists.
- **`tests/proto_workflow_contracts.rs`** — `proto/` directory exists.

## Related

- [Workflow JSON Schemas (feature)](../../../../docs/ft/coder/workflow-json-schemas.md)  
- [Workflow recipes (feature)](../../../../docs/ft/coder/workflow-recipes.md)  
- `docs/dev/1-WIP/workflow-schema-pipeline.md` — contributor edit workflow  

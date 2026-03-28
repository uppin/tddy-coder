# Workflow schema pipeline

## Overview

The pipeline keeps **one registry** (`packages/tddy-workflow-recipes/goals.json`) for each workflow goal’s CLI name, JSON Schema filename, and Protocol Buffer filename. The **`tddy-workflow-recipes`** build copies `schemas/**/*.json` into **`generated/`**, writes **`schema-manifest.json`**, and emits **`proto_basenames.rs`**. The **`tddy-tools`** build reads the same **`goals.json`** and emits **`goal_registry.rs`** so the binary embeds the correct goal → file mapping.

JSON Schema files under **`schemas/`** are the validation source embedded in **`tddy-tools`**. Protocol Buffer files under **`proto/`** document the same contracts at the IDL layer; a future **proto → JSON Schema** codegen step would replace the copy-only sync between IDL and schema files when that toolchain is selected.

## Editing contracts

1. Edit **`goals.json`** together with the matching **`.schema.json`** and **`.proto`** files.
2. Run **`cargo build -p tddy-workflow-recipes`** (or any build that compiles **`tddy-tools`**) so **`generated/`** refreshes.
3. Run **`cargo test -p tddy-tools -p tddy-workflow-recipes`**.

## Automated checks

- **`schema_manifest::manifest_goal_names_match_goal_registry`** — embedded manifest matches **`goal_registry.rs`**.
- **`goals_contract`** — **`goals.json`** aligns with **`schema-manifest.json`** and proto basename count.
- **`proto_goal_files`**, **`proto_workflow_contracts`** — proto files and **`proto/`** root exist.

## References

- Feature: [Workflow JSON Schemas](../../ft/coder/workflow-json-schemas.md)  
- `packages/tddy-tools/docs/json-schema.md`  
- `packages/tddy-workflow-recipes/docs/workflow-schemas.md`  

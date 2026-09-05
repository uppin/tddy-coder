# 2026-03-28 — Workflow JSON Schemas (tddy-tools + tddy-workflow-recipes)

- **Registry**: `packages/tddy-workflow-recipes/goals.json` lists each CLI goal with schema filename and proto basename; build output includes `generated/schema-manifest.json` and generated proto basename tables.
- **tddy-tools**: Embeds schemas from `tddy-workflow-recipes/generated/`; subcommands `get-schema`, `list-schemas`, and validated `submit`; 16 MiB cap on stdin/`--data` for submit and ask.
- **Documentation**: [workflow-json-schemas.md](../workflow-json-schemas.md), package notes under `packages/tddy-tools/docs/json-schema.md` and `packages/tddy-workflow-recipes/docs/workflow-schemas.md`.

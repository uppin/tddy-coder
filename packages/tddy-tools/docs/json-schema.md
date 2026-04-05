# JSON Schema embedding and CLI (`tddy-tools`)

## Purpose

The library embeds workflow JSON Schemas from **`tddy-workflow-recipes/generated/`** and validates structured output before relaying to **`tddy-coder`**. Goal names and schema filenames originate from **`packages/tddy-workflow-recipes/goals.json`**; the build script emits **`OUT_DIR/goal_registry.rs`** with `GOAL_SCHEMA_FILES`.

## Modules

| Module | Responsibility |
|--------|----------------|
| `schema` | Embedded tree, `get_schema`, `validate_output`, common resource registration for `$ref`, `write_schema_to_path` |
| `schema_manifest` | Parses embedded `schema-manifest.json` for `list_registered_goals()` |

## CLI

| Subcommand | Behavior |
|------------|----------|
| `submit` | Optional `--goal`, `--data` / `--data-stdin`; validates when a known goal schema exists |
| `get-schema <goal>` | Prints schema JSON; `-o` writes goal file and `common/` subtree |
| `list-schemas` | Prints `{"goals":[...]}` |
| `ask` | Clarification relay (separate JSON schema) |
| `set-session-context` | Merges JSON into `.workflow/<id>.session.json` (`TDDY_SESSION_DIR`, `TDDY_WORKFLOW_SESSION_ID`); not listed in `goals.json` |
| `persist-changeset-workflow` | `--session-dir`, `--data` — validates JSON against **`changeset-workflow`**, writes **`workflow`** on **`changeset.yaml`** atomically; listed in `goals.json` for schema embedding |

## Logging

`env_logger` initializes at startup (default level **warn**; use `RUST_LOG` for `info` / `debug`). Validation and schema resolution use scoped `log` targets under `tddy_tools::schema` and `tddy_tools::schema_manifest`.

## Testing

Integration tests under `packages/tddy-tools/tests/` cover CLI behavior and schema validation fixtures; `schema_validation_tests.rs` asserts parity between source `schemas/red.schema.json` and `generated/red.schema.json` in **`tddy-workflow-recipes`**.

## Related packages

- **`tddy-workflow-recipes`** — `goals.json`, `schemas/`, `proto/`, `build.rs`, generated artifacts  

# 2026-03-12 — Schema via tddy-tools (No Schema Files)

- **Schema ownership**: All schema logic moved to tddy-tools. tddy-core no longer has a schema module, does not write schemas to disk, and does not depend on jsonschema or include_dir.
- **submit**: `tddy-tools submit --goal <goal> --data '<json>'` validates against embedded schemas. No `--schema` file path.
- **get-schema**: New subcommand outputs JSON schema for a goal. Optional `-o <path>` writes to file.
- **Validation error tips**: When validation fails, tddy-tools prints a tip to run `tddy-tools get-schema <goal>`.
- **System prompts**: All goals instruct the agent to use `tddy-tools submit --goal X` and `tddy-tools get-schema X` for format inspection.
- **Packages**: tddy-tools (schemas/, schema.rs, get-schema, --goal), tddy-core (schema module removed, ProcessToolExecutor uses --goal, tdd_hooks no write_schema_to_dir).

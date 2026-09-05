# 2026-03-12 — Schema via tddy-tools

**Type:** Refactor

Removed schema module from tddy-core. All schema logic (embedded schemas, validation, get-schema) lives in tddy-tools. No schema files written to disk. ProcessToolExecutor invokes `tddy-tools submit --goal <goal> --data '<json>'`. tdd_hooks no longer calls write_schema_to_dir. Removed include_dir and jsonschema dependencies. System prompts instruct agent to use `tddy-tools submit --goal X` and `tddy-tools get-schema X`. (tddy-core)

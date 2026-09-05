# 2026-03-08 — JSON Schema Structured Output Validation

**Type:** Feature

Formal JSON Schema files for all 7 goals (plan, acceptance-tests, red, green, validate, evaluate, validate-refactor) with common types ($ref). Schemas embedded via include_dir, written to plan dir for agent Read. schema::validate_output() validates before serde. On validation failure: 1 retry with errors + schema path. extract_last_structured_block parses schema= attribute. validate_and_retry in workflow. System prompts reference schema path and include schema= in example. Dependencies: jsonschema (default-features=false), include_dir. (tddy-core)

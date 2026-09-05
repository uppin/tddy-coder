# 2026-03-08 — JSON Schema Structured Output Validation

- **Schema files**: Formal JSON Schema files for all 7 goals (plan, acceptance-tests, red, green, validate, evaluate, validate-refactor) with shared types via `$ref` in `schemas/common/`.
- **Embedding**: Schemas embedded in binary via `include_dir`; written to `{plan-dir}/schemas/` for agent Read tool.
- **Working directory**: Agent runs with working_dir = plan_dir for plan, acceptance-tests, red, green, validate-refactor so `schemas/xxx.schema.json` resolves to `{plan-dir}/schemas/xxx.schema.json`. Validate and evaluate use working_dir for schema location.
- **Validation**: Agent output validated against schema before serde deserialization. On failure: 1 retry with validation errors and schema path in prompt.
- **Explicit contract**: `<structured-response schema="schemas/red.schema.json">` attribute declares expected format. System prompts reference schema path and include `schema=` in examples.
- **Tests**: Fixtures for valid and invalid JSON per goal; retry integration tests (invalid→valid succeeds; invalid twice→Failed).

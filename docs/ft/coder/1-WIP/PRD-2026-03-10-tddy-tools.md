# PRD: tddy-tools — Generic Tool Calling Handler

**Date**: 2026-03-10
**Status**: Draft
**Type**: Feature Change (modifying existing)

## Affected Features

- [Planning Step](../planning-step.md) — structured output and clarification questions
- [Implementation Step](../implementation-step.md) — structured output for red, green, evaluate, validate, refactor goals

## Summary

Repurpose `tddy-permission` (currently a minimal MCP server for Claude Code permission prompts) into `tddy-tools` — a generic tool calling handler that both handles MCP and accepts agent-friendly CLI tool calls.

Replace the current inline structured output mechanism (`<structured-response>` XML blocks with embedded JSON, `<clarification-questions>` blocks, `AskUserQuestion` tool events) with explicit CLI tool calls to `tddy-tools`. The agent is instructed to call the binary directly, passing data and an adhering JSON schema, instead of embedding structured JSON in its text output.

## Background

The current architecture instructs LLM agents to embed structured JSON inside custom XML-like tags (`<structured-response>`, `<clarification-questions>`) in their text output. This approach is fragile:

1. **Parsing complexity**: The output parser (~1600 lines) must handle edge cases — system prompt examples confused with actual output, multiple blocks requiring "pick the last one" heuristics, invalid JSON, empty blocks
2. **No standard tool interface**: The agent produces inline text that is post-hoc parsed, rather than calling a tool with structured input
3. **Token waste**: Schema examples embedded in system prompts consume tokens; agents must learn a custom output format per goal
4. **Backend divergence**: Claude and Cursor have different mechanisms for tool events, requiring separate stream parsers with duplicated logic

Industry consensus (early 2026) shows well-designed CLIs are the preferred agent tool interface — they are token-efficient, composable, and agents are already trained on CLI patterns.

## Requirements

### R1: Rename and Repurpose the Binary

Rename `tddy-permission` package/binary to `tddy-tools`. The binary supports two modes:

- **CLI mode** (default): Subcommands (`submit`, `ask`) for agent-friendly tool calls
- **MCP mode** (`tddy-tools --mcp`): Retains the existing MCP stdio transport server for backwards compatibility with Claude Code's `--permission-prompt-tool`

### R2: `submit` Subcommand — Structured Output Submission

The agent calls `tddy-tools submit` to deliver structured output instead of embedding it inline.

```
tddy-tools submit --schema schemas/plan.schema.json < data.json
tddy-tools submit --schema schemas/plan.schema.json --data '{"goal":"plan","prd":"...","todo":"..."}'
```

- Accepts JSON data via stdin (pipe) or `--data` flag
- `--schema` passes the schema file path for validation
- Validates input against the provided JSON schema
- On success: outputs JSON result to stdout (e.g. `{"status":"ok","goal":"plan"}`) with exit code 0
- On validation error: outputs JSON error to stdout (e.g. `{"status":"error","errors":[...]}`) with non-zero exit code
- Human-readable messages go to stderr

### R3: `ask` Subcommand — Clarification Questions

The agent calls `tddy-tools ask` to submit clarification questions instead of using `AskUserQuestion` tool events or inline `<clarification-questions>` blocks.

```
tddy-tools ask --data '{"questions":[{"header":"Scope","question":"Which modules?","options":[...]}]}'
tddy-tools ask < questions.json
```

- Same input flexibility (stdin or `--data`)
- Questions follow the existing `ClarificationQuestion` JSON structure
- Result on stdout: JSON with question receipt confirmation
- Exit code 0 on success, non-zero on malformed input

### R4: Agent-Friendly CLI Design

The binary follows agent-friendly CLI principles:

- `--json` / structured JSON output to stdout by default
- Human-readable messages to stderr only
- Semantic exit codes: 0=success, 1=general failure, 2=usage error, 3=validation error
- Comprehensive `--help` with examples
- Non-interactive (no prompts, no spinners)
- Idempotent operations
- Consistent noun-verb grammar (`tddy-tools submit`, `tddy-tools ask`)

### R5: System Prompt Migration

Update all goal system prompts (plan, acceptance-tests, red, green, evaluate, validate-subagents, refactor) to instruct the agent to call `tddy-tools submit` instead of embedding `<structured-response>` blocks. The agent should:

1. Construct the JSON output
2. Call `tddy-tools submit --schema <path> --data '<json>'` (or pipe via stdin)
3. Check the tool call result for validation errors
4. If validation fails, fix the JSON and retry

### R6: Remove Inline Parsing

Once agents use CLI tool calls, the `<structured-response>` and `<clarification-questions>` parsing paths in `output/parser.rs` and `stream/mod.rs` can be deprecated. The output from `tddy-tools submit` replaces the inline parsing pipeline.

### R7: IPC Communication

`tddy-tools` needs to communicate the submitted data back to the running `tddy-coder` workflow. The mechanism should be:

- File-based IPC: `tddy-tools submit` writes the validated JSON to a known file path in the plan directory (e.g. `<plan_dir>/.toolcall-result.json`)
- The workflow runner polls or watches for this file
- Alternatively, `tddy-tools` can use the existing gRPC channel if available

## Acceptance Criteria

1. `tddy-tools submit --schema <path> --data '<json>'` validates and returns structured result
2. `tddy-tools submit --schema <path> < data.json` works via stdin pipe
3. `tddy-tools ask --data '<json>'` accepts clarification questions
4. `tddy-tools --mcp` launches the MCP server (backwards compatible)
5. `tddy-tools --help` and `tddy-tools submit --help` provide comprehensive help text
6. Exit codes are semantic (0, 1, 2, 3)
7. All goal system prompts instruct agents to call `tddy-tools submit` instead of inline blocks
8. The workflow receives submitted data via IPC mechanism
9. Existing tests continue to pass (backwards compatibility during transition)

## Testing Plan

### Test Level: Integration + Unit

**Rationale**: The CLI interface is the contract between the agent and tddy-tools. Integration tests verify end-to-end CLI behavior (invoke binary, check stdout/exit code). Unit tests verify schema validation logic and JSON parsing.

### Acceptance Tests

1. **`submit_valid_json_with_schema_returns_success`** — Submit valid plan JSON with plan schema, expect exit code 0 and success JSON on stdout
2. **`submit_invalid_json_returns_validation_error`** — Submit JSON missing required fields, expect exit code 3 and error details on stdout
3. **`submit_reads_from_stdin`** — Pipe JSON via stdin, verify same behavior as --data
4. **`submit_malformed_json_returns_parse_error`** — Submit non-JSON data, expect exit code 1
5. **`ask_valid_questions_returns_success`** — Submit valid clarification questions JSON, expect exit code 0
6. **`ask_malformed_input_returns_error`** — Submit invalid questions format, expect non-zero exit
7. **`mcp_mode_launches_server`** — `tddy-tools --mcp` starts MCP server (backwards compatible)
8. **`help_text_is_comprehensive`** — `--help` output includes examples and flag descriptions
9. **`schema_file_not_found_returns_error`** — Reference nonexistent schema file, expect clear error
10. **`submit_writes_result_to_plan_dir`** — After successful submit, result file exists in plan directory

### Target Test Files

- `packages/tddy-tools/tests/cli_integration.rs` (new)
- `packages/tddy-tools/src/cli.rs` (unit tests for argument parsing)
- `packages/tddy-core/tests/schema_validation_tests.rs` (existing, may need updates)

## References

- [Current permission.rs](../../packages/tddy-core/src/permission.rs) — goal allowlists
- [Current output/parser.rs](../../packages/tddy-core/src/output/parser.rs) — inline parsing to be replaced
- [Current stream/mod.rs](../../packages/tddy-core/src/stream/mod.rs) — question extraction
- [Schema module](../../packages/tddy-core/src/schema/mod.rs) — embedded JSON schemas

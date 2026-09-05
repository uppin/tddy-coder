# 2026-03-11 — tddy-tools Submit Only (Drop Inline Parsing)

- **Sole output mechanism**: `tddy-tools submit` via Unix socket is the only way agents deliver structured output. All inline parsing (XML `<structured-response>` blocks, `---PRD_START---`/`---PRD_END---` delimiters, raw JSON prefix checks) has been removed from `output/parser.rs`.
- **Parser simplification**: Each `parse_*_response()` function accepts pre-validated JSON from `tddy-tools submit` and deserializes into typed structs. No text scanning, XML parsing, or delimiter matching.
- **Fail-fast**: When the agent finishes without calling `tddy-tools submit`, the workflow fails immediately with a clear diagnostic (e.g., "Agent finished without calling tddy-tools submit. Ensure tddy-tools is on PATH.").
- **Binary verification**: `tddy-tools` availability is verified at startup before starting any workflow. Fails early if not found.
- **Stream parsing**: Removed `<structured-response>` handling from `stream/mod.rs` and `stream/claude.rs`. Clarification questions still come from `AskUserQuestion` tool events.
- **System prompts**: All goal system prompts (plan, acceptance-tests, red, green, evaluate, validate, refactor, update-docs) instruct the agent to call `tddy-tools submit` with the appropriate schema path.
- **Packages**: tddy-core (parser.rs JSON-only, stream cleanup, fail-fast in PlanTask/BackendInvokeTask), tddy-coder (verify_tddy_tools_available at startup, stub agent option).

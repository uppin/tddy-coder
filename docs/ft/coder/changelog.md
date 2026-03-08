# Coder Changelog

Release note history for the Coder product area.

## 2026-03-08 — Context Header for Agent Prompts

- **Context reminder**: Plan, acceptance-tests, and red prompts are prepended with a `<context-reminder>` block listing absolute paths to existing .md artifacts (PRD.md, TODO.md, acceptance-tests.md, etc.) when the plan directory contains them.
- **Format**: Header starts with `**CRITICAL FOR CONTEXT AND SUMMARY**`; each line is `{filename}: {absolute_path}`. Omitted when plan dir is empty or no .md files exist.
- **Agent awareness**: Agents receive immediate visibility of available plan context files without discovering them.

## 2026-03-08 — Plan Directory Relocation (plan_dir_suggestion)

- **Agent-decided location**: When the plan agent returns `plan_dir_suggestion` in discovery, the workflow relocates the plan directory from staging (output_dir) to the suggested path relative to the git root (e.g. `docs/dev/1-WIP/2026-03-08-feature/`).
- **Exit output**: On successful exit, tddy-coder prints the plan directory path (plan, acceptance-tests, red, green goals and full workflow).
- **Resume**: Full workflow resume requires `--plan-dir`; automatic discovery removed.
- **Validation**: Invalid suggestions (absolute paths, `..`, empty) fall back to staging location. Cross-device moves use copy-then-delete when rename fails.

## 2026-03-08 — JSON Schema Structured Output Validation

- **Schema files**: Formal JSON Schema files for all 7 goals (plan, acceptance-tests, red, green, validate, evaluate, validate-refactor) with shared types via `$ref` in `schemas/common/`.
- **Embedding**: Schemas embedded in binary via `include_dir`; written to `{plan-dir}/schemas/` for agent Read tool.
- **Working directory**: Agent runs with working_dir = plan_dir for plan, acceptance-tests, red, green, validate-refactor so `schemas/xxx.schema.json` resolves to `{plan-dir}/schemas/xxx.schema.json`. Validate and evaluate use working_dir for schema location.
- **Validation**: Agent output validated against schema before serde deserialization. On failure: 1 retry with validation errors and schema path in prompt.
- **Explicit contract**: `<structured-response schema="schemas/red.schema.json">` attribute declares expected format. System prompts reference schema path and include `schema=` in examples.
- **Tests**: Fixtures for valid and invalid JSON per goal; retry integration tests (invalid→valid succeeds; invalid twice→Failed).

## 2026-03-07 — Validate-Changes Goal

- **New goal**: `--goal validate-changes` analyzes current git changes for risks (build validity, test infrastructure, production code quality, security). Produces validation-report.md in working directory.
- **Standalone**: Callable from Init without prior plan/red/green. Optional `--plan-dir` for changeset/PRD context. Uses fresh session (not resumed).
- **Permission**: validate_allowlist permits Read, Glob, Grep, SemanticSearch, git diff/log, find, cargo build/check.
- **State**: Init → Validating → Validated. Not in next_goal_for_state auto-sequence.
- **CLI**: `--conversation-output <path>` writes raw agent bytes in real time (each line appended as received).

## 2026-03-07 — Conversation Logging

- **CLI**: `--conversation-output <path>` logs the entire agent conversation in raw bytes to the specified file. Each NDJSON line is written in real time as it is received, so you can tail the file during long runs.

## 2026-03-07 — Backend Abstraction (OCP)

- **Backends**: Claude Code CLI and Cursor agent supported. Use `--agent claude` (default) or `--agent cursor`
- **CLI**: `--agent <name>` selects backend; `--prompt <text>` provides feature description (alternative to stdin)
- **Architecture**: InvokeRequest slimmed (Goal enum, no Claude-specific fields). InvokeResponse.session_id optional. Stream parsing split per backend (stream/claude.rs, stream/cursor.rs)
- **changeset.yaml**: Session entries include `agent` field for resume

## 2026-03-07 — Full Workflow When --goal Omitted

- **Full workflow**: When `--goal` is omitted, tddy-coder runs plan → acceptance-tests → red → green in a single invocation
- **Resume**: Auto-detects completed state from `changeset.yaml`; re-running skips completed steps (via `--plan-dir` or scanning `--output-dir`)
- **CLI**: `--goal` is now optional; individual goals (`plan`, `acceptance-tests`, `red`, `green`) unchanged
- **Output**: Full workflow prints green step output on success; when `GreenComplete`, re-running exits with summary

## 2026-03-10 — Goal Enhancements

- **changeset.yaml**: Replaces `.session` and `.impl-session` as the unified manifest. Contains name (PRD name from plan agent), initial_prompt, clarification_qa, sessions (with system_prompt_file per session), state, models, discovery, artifacts.
- **Plan goal**: Project discovery (toolchain, scripts, doc locations, relevant code). Demo planning (demo-plan.md). Agent decides PRD name. Stores initial_prompt and clarification_qa in changeset.yaml.
- **Observability**: Each goal displays agent and model before execution. State transitions displayed.
- **System prompts**: Stored in plan directory (e.g. system-prompt-plan.md); referenced per-session via system_prompt_file in changeset.yaml.
- **Green goal**: Executes demo plan when demo-plan.md exists; writes demo-results.md.
- **Model resolution**: Goals use model from changeset.yaml when --model not specified; CLI --model overrides.

## 2026-03-07 — Green Goal & Implementation Step

- **Green goal**: `--goal green --plan-dir <path>` resumes red session via `.impl-session`, implements production code to make failing tests pass, updates progress.md and acceptance-tests.md
- **Red goal**: Now persists session ID to `.impl-session` for green to resume
- **State machine**: New states GreenImplementing, GreenComplete
- **Documentation**: Red and green moved to `implementation-step.md`; `planning-step.md` covers only plan and acceptance-tests
- **CLI**: `--goal green` requires `--plan-dir`

## 2026-03-07 — Red Goal & Acceptance-Tests.md

- **Red goal**: `--goal red --plan-dir <path>` reads PRD.md and acceptance-tests.md, creates skeleton production code and failing lower-level tests via Claude
- **acceptance-tests.md**: acceptance-tests goal now writes acceptance-tests.md (structured list + rich descriptions) to the plan directory
- **State machine**: New states RedTesting, RedTestsReady
- **CLI**: `--goal red` requires `--plan-dir`

## 2026-03-07 — Permission Handling in Claude Code Print Mode

- **Print mode constraint**: tddy-coder uses Claude Code in print mode (`-p`); stdin is not used for interactive permission prompts
- **Hybrid policy**: Each goal has a predefined allowlist passed as `--allowedTools`; plan: Read, Glob, Grep, SemanticSearch; acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch
- **CLI**: `--allowed-tools` adds extra tools to the goal allowlist; `--debug` prints Claude CLI command and cwd
- **tddy-permission crate**: MCP server with `approval_prompt` tool for unexpected permission requests (TTY IPC deferred)

## 2026-03-07 — Acceptance Tests Goal

- **New goal**: `--goal acceptance-tests --plan-dir <path>` reads a completed plan, resumes the Claude session, creates failing acceptance tests, and verifies they fail
- **Session persistence**: Plan goal now writes `.session` file for session resumption
- **Testing Plan in PRD**: Plan system prompt requires a Testing Plan section (test level, acceptance tests list, target files, assertions)
- **State machine**: New states `AcceptanceTesting` and `AcceptanceTestsReady`
- **CLI**: `--plan-dir` flag required for acceptance-tests goal

## 2026-03-07 — Claude Stream-JSON Backend

- **Output format**: Switched from plain text to NDJSON stream (`--output-format=stream-json`)
- **Session management**: `--session-id` on first call, `--resume` on Q&A followup for context continuity
- **Structured Q&A**: Questions from `AskUserQuestion` tool events; inquire Select/MultiSelect prompts
- **Real-time progress**: Tool activity display (Read, Glob, Bash, etc.)
- **Output parsing**: Structured-response format (`<structured-response content-type="application-json">`) with delimiter fallback
- **CLI**: `--agent-output` flag for raw agent output to stderr; goal-specific exit prints PRD path to stdout

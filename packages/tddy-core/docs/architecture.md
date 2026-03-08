# Architecture

## Overview

tddy-core provides the core library for the tddy-coder TDD workflow orchestrator. It defines the `CodingBackend` trait for LLM backends, the `Workflow` state machine, NDJSON stream parsing for Claude Code CLI output, output parsing for PRD/TODO (structured-response and delimited formats), artifact writing, and changeset.yaml persistence.

## Components

### Backend (`backend/`)

- **CodingBackend**: Trait for invoking LLM-based coders. Implementations: `ClaudeCodeBackend`, `CursorBackend` (production), `MockBackend` (testing). `AnyBackend` enum for CLI dispatch.
- **InvokeRequest/InvokeResponse**: Request and response types. InvokeRequest: prompt, system_prompt, goal (Plan/AcceptanceTests/Red/Green/Validate/Evaluate/ValidateRefactor), model, session_id, is_resume, working_dir, debug, agent_output, inherit_stdin, extra_allowed_tools, conversation_output_path. InvokeResponse: output, exit_code, session_id (Option), questions. CursorBackend rejects Goal::ValidateRefactor (requires Agent tool, Claude-only).
- **ClarificationQuestion**: Structured question type from AskUserQuestion tool events or `<clarification-questions>` text block (header, question, options, multi_select).
- **ClaudeInvokeConfig**: Claude-specific config (permission_mode, allowed_tools, permission_prompt_tool, mcp_config_path) derived from goal internally.

### Changeset (`changeset.rs`)

- **Changeset**: Unified manifest in plan directory. Replaces `.session` and `.impl-session`. Contains name, initial_prompt, clarification_qa, models, sessions (with system_prompt_file per session), state, artifacts, discovery.
- **SessionEntry**: id, agent, tag, created_at, system_prompt_file (path to system prompt for this session).
- **ClarificationQa**: Question and answer pairs from planning clarification.
- **read_changeset / write_changeset**: Load and persist changeset.yaml.
- **append_session_and_update_state**: Add session (agent from backend.name(), id, tag, system_prompt_file); update workflow state.

### Stream (`stream/`)

- **stream/claude.rs**: `process_ndjson_stream` — Claude Code CLI NDJSON parser (assistant, user, result, tool_use, task_started, task_progress). Tool_result content from user events is collected separately and merged into result_text only as a fallback when primary sources (assistant text, result event) lack a structured-response block.
- **stream/cursor.rs**: `process_cursor_stream` — Cursor agent NDJSON parser (assistant, tool_call, result; askUserQuestionToolCall/askQuestionToolCall).
- **StreamResult**: result_text, session_id, questions, raw_lines.
- **ProgressEvent**: ToolUse, TaskStarted, TaskProgress for real-time display.
- **parse_clarification_questions_from_text**: Fallback when agent outputs `<clarification-questions>` block instead of AskUserQuestion tool.

### Permission (`permission.rs`)

- **plan_allowlist / acceptance_tests_allowlist / red_allowlist / green_allowlist / validate_allowlist / evaluate_allowlist / validate_refactor_allowlist**: Goal-specific tool allowlists passed as `--allowedTools`. Plan: Read, Glob, Grep, SemanticSearch. Acceptance-tests, Red, Green: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch. Validate, Evaluate: Read, Glob, Grep, SemanticSearch, Bash(git diff/log/find/cargo build/check *). ValidateRefactor: Agent, Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(git diff/cargo build/check/test *).

### Workflow (`workflow/`)

- **WorkflowState**: Init, Planning, Planned, AcceptanceTesting, AcceptanceTestsReady, RedTesting, RedTestsReady, GreenImplementing, GreenComplete, Validating, Validated, Evaluating, Evaluated, ValidateRefactorComplete, Failed.
- **Workflow**: Orchestrates plan, acceptance-tests, red, green, evaluate, and validate-refactor steps with session continuity for Q&A followup. Each goal calls `validate_and_retry` after invoke: validates JSON against schema, retries once with validation errors on failure.
- **planning**: System prompt (structured-response format) and user prompt construction. Writes system prompt to plan dir; stores initial_prompt and clarification_qa in changeset. Persists questions when ClarificationNeeded; pairs with answers on follow-up.
- **acceptance_tests**: System prompt for test creation and verification; parses test summary and run instructions; writes acceptance-tests.md; appends session to changeset.
- **red**: System prompt for skeleton code and failing lower-level tests; parses RedOutput; writes red-output.md and progress.md; appends impl session to changeset.
- **green**: System prompt for implementation; parses GreenOutput; updates progress.md and acceptance-tests.md; writes demo-results.md when demo plan exists.
- **validate**: Standalone goal for change validation. Reads git diff, runs build, produces validation-report.md. Uses fresh session (not resumed). Optional plan_dir for changeset/PRD context. State: Validating → Validated. Not in next_goal_for_state auto-sequence.
- **evaluate**: Analyzes git changes for risks, changed files, affected tests, and validity. Requires plan_dir; writes evaluation-report.md. Reads optional PRD.md and changeset.yaml for context. EvaluateOptions: model, agent_output, conversation_output_path, inherit_stdin, allowed_tools_extras, debug. State: Evaluating → Evaluated.
- **validate_refactor**: Orchestrates validate-tests, validate-prod-ready, and analyze-clean-code subagents via the Agent tool. Requires evaluation-report.md in plan_dir (from prior evaluate run). Claude-only (CursorBackend rejects). ValidateRefactorOptions: model, agent_output, conversation_output_path, inherit_stdin, allowed_tools_extras, debug. State: → ValidateRefactorComplete.

### Schema (`schema/`)

- **JSON Schema validation**: Formal schemas in `schemas/` (7 goals + common types). Embedded via `include_dir`, written to `{plan-dir}/schemas/` for agent Read. Agent's working directory is plan_dir (or working_dir for validate/evaluate), so `schemas/xxx.schema.json` resolves correctly. `validate_output(goal, json)` validates before serde. On failure: 1 retry with validation errors + schema path in prompt.
- **get_schema / write_all_schemas_to_dir / write_schema_to_dir / format_validation_errors**: Schema retrieval. `write_all_schemas_to_dir` called when plan dir is created; writes all 7 goal schemas + common types so subsequent goals (acceptance-tests, red, green, validate-refactor) have schemas available. `write_schema_to_dir` used for validate/evaluate (working_dir may differ from plan_dir). Error formatting for retry prompts.

### Output (`output/`)

- **extract_last_structured_block**: Extracts last `<structured-response>` block and optional `schema="..."` attribute. Used for validation before parsing.
- **parse_planning_response**: Extracts PRD and TODO from structured-response or delimited text. Tries each structured-response block until one parses (handles system prompt example before model output).
- **parse_acceptance_tests_response**: Extracts test summary, test_command, prerequisite_actions, run_single_or_selected_tests from acceptance-tests response.
- **parse_red_response**: Extracts RedOutput (summary, tests, skeletons, markers, marker_results, run instructions) from red goal response. Uses last structured-response block (handles system prompt example before model output).
- **parse_green_response**: Extracts GreenOutput (summary, tests, demo_results) from green goal response.
- **write_artifacts**: Writes PRD.md, TODO.md, demo-plan.md to the plan directory.
- **write_acceptance_tests_file / write_red_output_file / write_progress_file / write_demo_results_file / write_validation_report**: Artifact writers.
- **parse_validate_response**: Extracts ValidateOutput (summary, risk_level, build_results, issues, changeset_sync, files_analyzed, test_impact) from validate-changes goal response.
- **parse_evaluate_response**: Extracts EvaluateOutput (summary, risk_level, build_results, issues, changeset_sync, files_analyzed, test_impact, changed_files, affected_tests, validity_assessment) from evaluate-changes goal response. Uses rfind to locate last structured-response block.
- **parse_validate_refactor_response**: Extracts ValidateRefactorOutput (goal, summary, tests_report_written, prod_ready_report_written, clean_code_report_written) from validate-refactor goal response.
- **write_evaluation_report**: Writes evaluation-report.md to plan_dir from EvaluateOutput.
- **slugify_directory_name**: Generates directory names (YYYY-MM-DD-<slug>).

## Data Flow

```
Input → Workflow::plan() → Backend::invoke() → stream::process_ndjson_stream() → Parse → Write → Output path
         ↑                        ↓
         └── ClarificationNeeded (questions) ← AskUserQuestion tool events
         └── On success: write changeset.yaml (initial_prompt, clarification_qa, sessions)
```

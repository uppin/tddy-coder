# Architecture

## Overview

tddy-core provides the core library for the tddy-coder TDD workflow orchestrator. It defines the `CodingBackend` trait for LLM backends, the `Workflow` state machine, NDJSON stream parsing for Claude Code CLI output, output parsing for PRD/TODO (structured-response and delimited formats), and artifact writing.

## Components

### Backend (`backend/`)

- **CodingBackend**: Trait for invoking LLM-based coders. Implementations include `ClaudeCodeBackend` (production) and `MockBackend` (testing).
- **InvokeRequest/InvokeResponse**: Request and response types. Supports session_id, is_resume, agent_output, allowed_tools (goal allowlist for `--allowedTools`), permission_prompt_tool, mcp_config_path, working_dir, debug.
- **ClarificationQuestion**: Structured question type from AskUserQuestion tool events (header, question, options, multi_select).
- **PermissionMode**: Plan (read-only), AcceptEdits (auto-approve file edits), or Default.

### Stream (`stream/`)

- **process_ndjson_stream**: Parses Claude Code CLI `--output-format=stream-json` NDJSON output.
- **StreamResult**: result_text (accumulated assistant text + result), session_id, questions.
- **ProgressEvent**: ToolUse, TaskStarted, TaskProgress for real-time display.
- Extracts AskUserQuestion tool events for structured Q&A; deduplicates questions.

### Permission (`permission.rs`)

- **plan_allowlist / acceptance_tests_allowlist**: Goal-specific tool allowlists passed as `--allowedTools`. Plan: Read, Glob, Grep, SemanticSearch. Acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch.

### Workflow (`workflow/`)

- **WorkflowState**: Init, Planning, Planned, AcceptanceTesting, AcceptanceTestsReady, Failed.
- **Workflow**: Orchestrates the planning step and acceptance-tests step with session continuity for Q&A followup.
- **planning**: System prompt (structured-response format) and user prompt construction.
- **acceptance_tests**: System prompt for test creation and verification; parses test summary from response.

### Output (`output/`)

- **parse_planning_response**: Extracts PRD and TODO from structured-response (`<structured-response content-type="application-json">`) or delimited text.
- **parse_acceptance_tests_response**: Extracts test summary from acceptance-tests response.
- **write_artifacts**: Writes PRD.md and TODO.md to the filesystem.
- **write_session_file / read_session_file**: Session ID persistence for session resumption.
- **slugify_directory_name**: Generates directory names (YYYY-MM-DD-<slug>).

## Data Flow

```
Input → Workflow::plan() → Backend::invoke() → stream::process_ndjson_stream() → Parse → Write → Output path
         ↑                        ↓
         └── ClarificationNeeded (questions) ← AskUserQuestion tool events
```

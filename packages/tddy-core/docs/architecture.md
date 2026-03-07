# Architecture

## Overview

tddy-core provides the core library for the tddy-coder TDD workflow orchestrator. It defines the `CodingBackend` trait for LLM backends, the `Workflow` state machine, NDJSON stream parsing for Claude Code CLI output, output parsing for PRD/TODO (structured-response and delimited formats), and artifact writing.

## Components

### Backend (`backend/`)

- **CodingBackend**: Trait for invoking LLM-based coders. Implementations include `ClaudeCodeBackend` (production) and `MockBackend` (testing).
- **InvokeRequest/InvokeResponse**: Request and response types. Supports session_id, is_resume, agent_output.
- **ClarificationQuestion**: Structured question type from AskUserQuestion tool events (header, question, options, multi_select).
- **PermissionMode**: Plan (read-only) or Default.

### Stream (`stream/`)

- **process_ndjson_stream**: Parses Claude Code CLI `--output-format=stream-json` NDJSON output.
- **StreamResult**: result_text (accumulated assistant text + result), session_id, questions.
- **ProgressEvent**: ToolUse, TaskStarted, TaskProgress for real-time display.
- Extracts AskUserQuestion tool events for structured Q&A; deduplicates questions.

### Workflow (`workflow/`)

- **WorkflowState**: Init, Planning, Planned, Failed.
- **Workflow**: Orchestrates the planning step with session continuity for Q&A followup.
- **planning**: System prompt (structured-response format) and user prompt construction.

### Output (`output/`)

- **parse_planning_response**: Extracts PRD and TODO from structured-response (`<structured-response content-type="application-json">`) or delimited text.
- **write_artifacts**: Writes PRD.md and TODO.md to the filesystem.
- **slugify_directory_name**: Generates directory names (YYYY-MM-DD-<slug>).

## Data Flow

```
Input → Workflow::plan() → Backend::invoke() → stream::process_ndjson_stream() → Parse → Write → Output path
         ↑                        ↓
         └── ClarificationNeeded (questions) ← AskUserQuestion tool events
```

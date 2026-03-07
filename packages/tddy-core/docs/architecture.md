# Architecture

## Overview

tddy-core provides the core library for the tddy-coder TDD workflow orchestrator. It defines the `CodingBackend` trait for LLM backends, the `Workflow` state machine, output parsing for delimited PRD/TODO content, and artifact writing.

## Components

### Backend (`backend/`)

- **CodingBackend**: Trait for invoking LLM-based coders. Implementations include `ClaudeCodeBackend` (production) and `MockBackend` (testing).
- **InvokeRequest/InvokeResponse**: Request and response types for backend invocations.
- **PermissionMode**: Plan (read-only) or Default.

### Workflow (`workflow/`)

- **WorkflowState**: Init, Planning, Planned, Failed.
- **Workflow**: Orchestrates the planning step: reads input, invokes backend, parses output, writes artifacts.
- **planning**: System prompt and user prompt construction.

### Output (`output/`)

- **parse_planning_output**: Extracts PRD and TODO from delimited text (`---PRD_START---` ... `---PRD_END---`, etc.).
- **write_artifacts**: Writes PRD.md and TODO.md to the filesystem.
- **slugify_directory_name**: Generates directory names (YYYY-MM-DD-<slug>).

## Data Flow

```
Input (feature description) → Workflow::plan() → Backend::invoke() → Parse → Write → Output path
```

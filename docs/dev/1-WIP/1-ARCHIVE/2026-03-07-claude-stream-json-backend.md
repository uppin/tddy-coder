# Changeset: Claude Stream-JSON Backend Overhaul

**Date**: 2026-03-07
**Status**: ✅ Complete
**Type**: Feature

## Affected Packages

- **tddy-core**: [README.md](../../packages/tddy-core/README.md) — Backend rewrite, NDJSON stream module, new types
- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md) — CLI Q&A flow, progress display

## Related Feature Documentation

- [PRD-2026-03-07-claude-stream-json-backend.md](../../ft/1-WIP/PRD-2026-03-07-claude-stream-json-backend.md)

## Summary

Replace the plain-text Claude backend with NDJSON stream processing (`--output-format=stream-json`), session management (`--session-id`/`--resume`), structured question extraction from `AskUserQuestion` tool events, and real-time progress display. Fixes deadlock bug and enables session continuity for Q&A.

## Background

The current backend uses a `script` wrapper with piped stdout that is never drained before `child.wait()`, causing deadlock when output exceeds the OS pipe buffer. Each invoke is independent; Q&A followup loses context. Questions are parsed from delimiter text instead of structured tool events.

## Scope

- [x] **Implementation**: Complete six milestones (NDJSON parser, backend types, Claude backend rewrite, workflow, output parsing, CLI)
- [x] **Testing**: All acceptance tests passing
- [x] **Production Readiness**: Validation complete

## Technical Changes

### State A (Current)

- `ClaudeCodeBackend` wraps `claude` with `script` command, pipes stdout, deadlocks
- Plain text output, delimiter-based parsing for PRD/TODO and questions
- No session management; each invoke is independent
- No real-time progress

### State B (Target)

- `ClaudeCodeBackend` invokes `claude` directly with `--output-format=stream-json`
- NDJSON stream parser extracts events, questions from tool_use, result text
- Session management: `--session-id` on first call, `--resume` on followup
- Progress callback for tool activity display

### Delta

#### tddy-core
- New `stream` module: event types, StreamProcessor, StreamResult
- `InvokeRequest`: add session_id, is_resume
- `InvokeResponse`: add session_id, questions (ClarificationQuestion)
- `ClaudeCodeBackend`: rewrite invoke(), add with_progress(), drop script
- `build_claude_args`: add --output-format stream-json, session flags
- `Workflow`: track session_id, use response.questions
- `WorkflowError::ClarificationNeeded`: structured questions, session_id
- `output/parser`: remove question delimiters, keep PRD/TODO
- Dependencies: serde, serde_json, uuid

#### tddy-coder
- Backend with progress callback
- Q&A flow uses ClarificationQuestion with inquire Select/MultiSelect
- Real-time progress display

## Implementation Milestones

- [x] Milestone 1: NDJSON event types and stream parser module
- [x] Milestone 2: Backend types evolution (InvokeRequest/Response, ClarificationQuestion)
- [x] Milestone 3: Rewrite ClaudeCodeBackend (drop script, direct pipe, stream processing)
- [x] Milestone 4: Update Workflow, error types, session tracking
- [x] Milestone 5: Update output parsing (remove question delimiters, keep PRD/TODO, add structured-response)
- [x] Milestone 6: Update CLI (inquire for structured questions, progress display) and all tests

## Acceptance Tests

### tddy-core
- [x] **Integration**: Stream processor parses NDJSON, extracts questions and result (stream_parsing.rs)
- [x] **Integration**: build_claude_args includes --output-format stream-json and session flags (backend_args.rs)
- [x] **Integration**: Planning workflow with MockBackend returns ClarificationNeeded with structured questions (planning_integration.rs)
- [x] **Integration**: Planning workflow produces PRD after Q&A with session resume (planning_integration.rs)
- [x] **Unit**: Output parser extracts PRD/TODO from result text and structured-response (output_parsing.rs)

### tddy-coder
- [x] **Integration**: CLI produces PRD+TODO with fake claude outputting NDJSON (cli_integration.rs)
- [x] **Integration**: CLI Q&A flow with NDJSON fake claude (cli_integration.rs)

## Testing Plan

**Primary Test Approach**: Integration tests with MockBackend (tddy-core) and fake claude scripts (tddy-coder).

**Strategy**: MockBackend returns pre-built NDJSON event sequences. Fake claude scripts echo NDJSON lines. Verify stream parsing, session args, workflow transitions, and CLI output.

## Technical Debt & Production Readiness

- None identified. Code quality validated via clippy and code review.

## Decisions & Trade-offs

- **Direct pipe over PTY**: `claude -p --output-format=stream-json` outputs structured text; no PTY needed.
- **Progress callback on backend**: Set at construction, does not change CodingBackend trait.
- **Tool-use-only for questions**: No fallback to delimiter parsing; AskUserQuestion is the source.

## Refactoring Needed

- None.

## Validation Results

- **cargo fmt**: Pass
- **cargo clippy -D warnings**: Pass
- **cargo test**: 39 tests pass (tddy-core: 7 unit + 28 integration, tddy-coder: 4 integration)
- **Code review**: StreamResult simplified (removed redundant exit_code), file_path_display helper extracted, #[must_use] on builder methods

## References

- [claude-json-output-plan.txt](../../../claude-json-output-plan.txt) — Real Claude stream-json output with AskUserQuestion
- [Baker CLI](../../../baker/packages/baker-cli) — Session resume pattern reference

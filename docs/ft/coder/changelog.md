# Coder Changelog

Release note history for the Coder product area.

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

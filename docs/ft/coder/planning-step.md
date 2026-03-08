# Planning Step — Feature Document

**Product Area**: Coder
**Status**: Draft
**Updated**: 2026-03-07

## Summary

The Planning Step is the first phase of the tddy-coder workflow. When `--goal` is omitted, tddy-coder runs the full workflow (plan → acceptance-tests → red → green) in a single invocation, with auto-resume from `changeset.yaml` state. When a specific goal is given, it executes that step only. The **plan** goal accepts a user's goal description via stdin or `--prompt`, invokes an LLM backend (Claude or Cursor per `--agent`) in plan mode, and produces a structured planning output: a named directory containing a `PRD.md` (Product Requirements Document), `TODO.md` (implementation task list), and `changeset.yaml` (unified manifest with session ID, workflow state, discovery, and models). The **acceptance-tests** goal reads a completed plan from `changeset.yaml`, resumes the Claude session, creates failing acceptance tests, writes `acceptance-tests.md`, and verifies they fail.

## Background

tddy-coder is a strict, state-machine-driven TDD workflow orchestrator. It uses LLM-based coders (starting with Claude Code) as backends to drive development from planning through production. The planning step is the entry point of the workflow — before any code is written, the system must produce a clear requirements document and a structured execution plan.

The tool treats the LLM as a subordinate: it instructs the LLM what to analyze, constrains its behavior via plan mode, and captures structured output. The LLM does not drive the workflow — the state machine does.

## Requirements

### CLI Interface (Updated: 2026-03-07)

1. Binary name: `tddy-coder`
2. When `--goal` is omitted, runs the full workflow (plan → acceptance-tests → red → green) with auto-resume from `changeset.yaml` state
3. Accepts `--goal plan` to trigger the planning step
4. Accepts `--goal acceptance-tests` to create failing acceptance tests from a completed plan
5. Accepts `--goal red` and `--goal green` for the implementation phase
6. Accepts `--allowed-tools <tools>` (comma-separated) to add extra tools to the goal's allowlist (e.g. `Bash(npm install)`)
7. Accepts `--plan-dir <path>`: path to plan output directory; required when `--goal acceptance-tests`, `--goal red`, or `--goal green`; used for resume when running full workflow
8. Accepts `--output-dir <path>` to configure where planning output is written (default: current directory)
9. Accepts `--model <name>` (or `-m <name>`) to select the LLM model (e.g. `opus`, `sonnet`, `haiku`)
10. Accepts `--agent-output` to print raw agent output to stderr in real time
11. Accepts `--conversation-output <path>` to log the entire agent conversation in raw bytes to a file (Updated: 2026-03-07)
12. Accepts `--debug` to print CLI command and cwd before running (for debugging empty output)
13. Accepts `--agent <name>` to select backend: `claude` (default) or `cursor`
14. Reads the feature description from stdin (supports piped input and interactive prompt), or from `--prompt <text>` when provided
15. *Deferred*: `--list-models` to list available models (not needed for current scope)

### Planning Workflow (Updated: 2026-03-07)

1. Read feature description from `--prompt` (if set) or stdin (piped or interactive)
2. Invoke the selected backend (Claude or Cursor) in plan mode to analyze the feature description
3. **Q&A phase**: The agent may ask clarifying questions; the user is expected to answer them. The system must support this interactive exchange (Claude asks → user answers → Claude continues analysis).
4. Generate a deterministic directory name based on the feature (date-prefixed, slugified)
5. Parse Claude Code's structured output into PRD, TODO, discovery, and demo plan artifacts
6. Write `PRD.md`, `TODO.md`, and `changeset.yaml` (unified manifest: session ID, workflow state, discovery, models, initial_prompt, clarification_qa) to the output directory
7. On successful exit, output the path to `PRD.md` (goal-specific exit output)

The PRD must include a **Testing Plan** section with: test level (E2E/Integration/Unit), list of acceptance tests, target test file paths, and strong assertions.

### Acceptance-Tests Workflow

1. Read PRD.md and TODO.md from the plan directory specified by `--plan-dir`
2. Read the session ID and model from `changeset.yaml` in the plan directory
3. Resume the Claude session using `--resume <session-id>`
4. Use `--permission-mode acceptEdits` (auto-approves file edits for creating tests and running `cargo test`)
5. **Q&A phase**: When Claude returns clarifying questions (e.g., permission requests), the user provides answers and the workflow continues
6. System prompt instructs Claude to: read the testing plan from the PRD; create acceptance tests as specified; verify all new tests fail (Red state); remove or adjust any tests that pass
7. Parse Claude's output to extract a summary of created tests and their status
8. Write `acceptance-tests.md` to the plan directory (structured list + rich descriptions for downstream goals)
9. On successful exit, output a human-readable summary (test count, paths, failing status)

### LLM Backend Abstraction (Updated: 2026-03-07)

1. The system defines a Rust trait (`CodingBackend`) for LLM interactions
2. Supported backends: Claude Code CLI and Cursor agent. Use `--agent claude` (default) or `--agent cursor` to select
3. The trait supports: invoking the LLM, passing prompts, receiving structured output
4. Backends support **model selection** (pass model name to the underlying CLI/API)
5. Tests use a mock implementation that allows test-controlled responses and behavior

### Claude Code Integration

1. Invokes `claude` CLI binary (from PATH)
2. **Print mode**: tddy-coder always uses `-p` (print mode) for non-interactive, single-query execution. In print mode, **stdin is not used for interactive permission prompts** — Claude Code handles permissions via `--permission-mode`, `--allowedTools`, or `--permission-prompt-tool`, not by reading user input from stdin.
3. **Plan goal**: Uses `--permission-mode plan` (read-only analysis) plus a predefined allowlist (`Read`, `Glob`, `Grep`, `SemanticSearch`) passed as `--allowedTools`.
4. **Acceptance-tests goal**: Uses `--permission-mode acceptEdits` plus a predefined allowlist (`Read`, `Write`, `Edit`, `Glob`, `Grep`, `Bash(cargo *)`, `SemanticSearch`) passed as `--allowedTools`.
5. **Hybrid permission policy**: Each goal has a built-in allowlist; tools matching the allowlist are auto-approved. Optional `--allowed-tools` CLI flag adds extra tools to the allowlist. Unexpected permission requests (not in the allowlist) are denied in non-interactive mode; interactive handling via embedded permission tool is available when enabled.
6. **Model selection**: Passes `--model <name>` to the `claude` binary. Model comes from `changeset.yaml` when `--model` is not specified; CLI `--model` overrides.
7. **Output format**: Uses `--output-format=stream-json` for NDJSON event stream (tool_use, result, task_progress).
8. **Session management**: First invoke uses `--session-id <uuid>`; Q&A followup uses `--resume <uuid>` so Claude retains context across the exchange. Session IDs are persisted in `changeset.yaml`.
9. **Structured Q&A**: Clarifying questions come from `AskUserQuestion` tool events (header, question, options, multi_select). Presented via inquire Select/MultiSelect prompts. Questions and answers are stored in `changeset.yaml` as `clarification_qa`.
10. **Real-time progress**: Tool activity (Read, Glob, Bash, etc.) displayed while Claude works.
11. **Output parsing**: System prompt instructs Claude to emit PRD and TODO in `<structured-response content-type="application-json">` format; parser also supports delimiter fallback.
12. **Structured output validation**: Each goal's output is validated against a JSON Schema file before serde deserialization. Schemas are embedded in the binary via `include_dir`, written to `{plan-dir}/schemas/` for the agent to read via its Read tool. The agent's working directory is the plan directory, so `schemas/plan.schema.json` resolves to `{plan-dir}/schemas/plan.schema.json`. System prompts reference the schema path and instruct the agent to emit the `schema="..."` attribute on the `<structured-response>` tag. On validation failure, the session resumes with validation errors (1 retry); the retry prompt includes the schema path and error details. (Updated: 2026-03-08)

### Project Discovery (Plan Goal)

The plan goal performs read-only discovery before producing the PRD:
- Parse `Cargo.toml`, `package.json`, `Makefile`, `flake.nix`, `.nvmrc`, `.tool-versions`, `.python-version`, `AGENTS.md` for tool/SDK versions and scripts
- Identify documentation locations (`docs/`, `packages/*/docs/`, README files)
- Discover the best location for the plan directory based on repo conventions
- Reveal relevant code areas (modules, traits, key files)
- Capture test infrastructure (runners, conventions, CI scripts)

Discovery is persisted in `changeset.yaml` for downstream goals.

### Demo Planning (Plan Goal)

The plan goal produces a demo plan based on project type detection:
- CLI: command invocations with expected output
- Web apps: browser navigation (URLs, interactions, expected states)
- Libraries: REPL/test-based demonstrations
- Storybook-enabled projects: storybook component views

Includes setup instructions and verification criteria. Written to `demo-plan.md` in the plan directory.

### Observability

- Each goal displays the agent and model before execution (e.g. `Using agent: claude, model: sonnet`)
- Each workflow state transition is displayed (e.g. `State: Init → Planning`)

### Output Artifacts

#### changeset.yaml

Unified manifest in the plan directory:
- `name`: One-liner PRD name (decided by the plan agent)
- `initial_prompt`: User's goal/feature description from stdin
- `clarification_qa`: Questions asked during planning and user's answers (empty if no clarification)
- `sessions`: Array of session entries (id, agent, tag, created_at, system_prompt_file)
- `state`: Current workflow state and history
- `models`: Model per goal (plan, acceptance-tests, red, green)
- `discovery`: Toolchain, scripts, doc locations, relevant code
- `artifacts`: Paths to PRD.md, TODO.md, acceptance-tests.md, etc.

System prompts are written to the plan directory (e.g. `system-prompt-plan.md`) and referenced per-session via `system_prompt_file`.

#### PRD.md

- Feature summary and background
- Requirements (functional and non-functional)
- Acceptance criteria with checkboxes
- Impact analysis (if applicable)

#### TODO.md

- Implementation milestones broken into discrete tasks
- Tasks ordered by dependency
- Each task has a clear "done" definition
- Status tracking (pending/in_progress/completed)

#### acceptance-tests.md

- Written by the acceptance-tests goal to the plan directory
- **How to run tests**: Command to run tests, derived from the project (e.g. `cargo test`, `npm test`, `pytest`)
- **Prerequisite actions**: What to do before running tests; uses the cheapest approach (e.g. "None" when the test command already builds)
- **How to run a single or selected tests**: Project-specific instructions (e.g. `cargo test <name>`, `pytest -k <pattern>`)
- Structured list (test name, file, line, status) for machine parsing
- Rich descriptions for LLM consumption in subsequent goals

### State Machine

1. The planning step is one state in the overall workflow state machine
2. **Plan goal**: Transitions `Init` → `Planning` → `Planned` (or `Failed`)
3. **Acceptance-tests goal**: Transitions `Init`/`Planned` → `AcceptanceTesting` → `AcceptanceTestsReady` (or `Failed`)
4. The state machine enforces that planning must complete before development begins
5. State transitions are explicit and auditable

### Exit Output

On successful completion, the program prints goal-specific output to stdout:

- **Full workflow** (no `--goal`): Green step output (summary, tests, implementations)
- **plan**: Path to `PRD.md` (e.g. `./2026-03-07-feature-slug/PRD.md`)
- **acceptance-tests**: Summary of created tests and their failing status (requires `--plan-dir`)
- **red**, **green**: Summary of created tests/skeletons or implementations

This enables scripting and piping (e.g. `tddy-coder --goal plan < feature.txt | xargs cat`).

### Full Workflow (No --goal)

When `--goal` is omitted, tddy-coder runs plan → acceptance-tests → red → green in sequence. Resume is supported: if interrupted, re-running (with the same `--output-dir` or explicit `--plan-dir`) skips completed steps by reading `changeset.yaml.state.current`. When state is `GreenComplete`, re-running exits with a summary.

## Acceptance Criteria

### Full Workflow (No --goal)

- [x] `tddy-coder` (no `--goal`) reads from stdin and runs plan → acceptance-tests → red → green
- [x] Full workflow prints green step output on success
- [x] Full workflow supports resume via `--plan-dir` or auto-detect from `--output-dir`
- [x] When state is GreenComplete, re-running exits with summary

### Plan Goal

- [x] `tddy-coder --goal plan` reads from stdin and produces a named output directory
- [ ] Output directory contains well-formed `PRD.md`, `TODO.md`, and `changeset.yaml`
- [ ] `--output-dir` flag controls output location
- [ ] `--model <name>` selects the LLM model; default used when omitted
- [ ] *Deferred*: `--list-models` lists available models
- [ ] Claude Code CLI is invoked in plan mode with appropriate arguments
- [ ] **Q&A support**: When Claude asks clarifying questions during planning, the user can provide answers and Claude continues analysis
- [ ] Plan system prompt produces a Testing Plan section in the PRD (test level, acceptance tests list, target files, assertions)
- [ ] CodingBackend trait enables mock-based testing without real Claude Code CLI
- [ ] Tests use a fake/mock backend to verify the planning workflow end-to-end
- [ ] Error cases handled: empty input, Claude Code not found, malformed LLM output
- [ ] State machine enforces valid transitions
- [ ] On successful plan completion, stdout prints the path to `PRD.md` (goal-specific exit output)

### Acceptance-Tests Goal

- [ ] `tddy-coder --goal acceptance-tests --plan-dir <path>` creates failing acceptance tests from a plan
- [ ] Acceptance-tests goal resumes the planning session via `--resume` for context continuity
- [ ] Claude runs tests and verifies all new tests fail (Red state); passing tests are adjusted or removed
- [ ] Output prints a summary of created tests, their paths, and failing status
- [ ] State machine transitions: Init/Planned → AcceptanceTesting → AcceptanceTestsReady
- [ ] Error handling: missing plan-dir, missing PRD.md, missing changeset.yaml, session resume failure
- [ ] `--model` and `--agent-output` flags work with the acceptance-tests goal
- [ ] acceptance-tests goal writes acceptance-tests.md to the plan directory

## Future Considerations (Not In Scope)

- Multi-turn refinement after initial plan (invoke → review → refine)
- File dependency analysis (Bazel-like)
- Test coverage and mutation testing integration
- Demo setup for user review
- Language-agnostic clean code analysis

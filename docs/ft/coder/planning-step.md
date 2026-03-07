# Planning Step — Feature Document

**Product Area**: Coder
**Status**: Draft
**Updated**: 2026-03-07

## Summary

The Planning Step is the first phase of the tddy-coder workflow. It accepts a user's goal description via stdin, invokes an LLM backend (Claude Code CLI) in plan mode, and produces a structured planning output: a named directory containing a `PRD.md` (Product Requirements Document), `TODO.md` (implementation task list), and `.session` (session ID for resumption). The **acceptance-tests** goal reads a completed plan, resumes the Claude session, creates failing acceptance tests, writes `acceptance-tests.md`, and verifies they fail. The **red** goal reads PRD and acceptance-tests.md, creates skeleton production code and failing lower-level tests.

## Background

tddy-coder is a strict, state-machine-driven TDD workflow orchestrator. It uses LLM-based coders (starting with Claude Code) as backends to drive development from planning through production. The planning step is the entry point of the workflow — before any code is written, the system must produce a clear requirements document and a structured execution plan.

The tool treats the LLM as a subordinate: it instructs the LLM what to analyze, constrains its behavior via plan mode, and captures structured output. The LLM does not drive the workflow — the state machine does.

## Requirements

### CLI Interface

1. Binary name: `tddy-coder`
2. Accepts `--goal plan` to trigger the planning step
3. Accepts `--goal acceptance-tests` to create failing acceptance tests from a completed plan
4. Accepts `--goal red` to create skeleton code and failing lower-level tests from PRD and acceptance-tests.md
5. Accepts `--allowed-tools <tools>` (comma-separated) to add extra tools to the goal's allowlist (e.g. `Bash(npm install)`)
6. Accepts `--plan-dir <path>`: path to plan output directory (PRD.md, TODO.md, .session, acceptance-tests.md); required when `--goal acceptance-tests` or `--goal red`
7. Accepts `--output-dir <path>` to configure where planning output is written (defaults to current directory)
8. Accepts `--model <name>` (or `-m <name>`) to select the LLM model (e.g. `opus`, `sonnet`, `haiku`)
9. Accepts `--agent-output` to print raw agent output to stderr in real time
10. Accepts `--debug` to print Claude CLI command and cwd before running (for debugging empty output)
11. Reads the feature description from stdin (supports piped input and interactive prompt)
12. *Deferred*: `--list-models` to list available models (not needed for current scope)

### Planning Workflow

1. Read feature description from stdin
2. Invoke Claude Code CLI in plan mode to analyze the feature description
3. **Q&A phase**: Claude Code may ask clarifying questions; the user is expected to answer them. The system must support this interactive exchange (Claude asks → user answers → Claude continues analysis).
4. Generate a deterministic directory name based on the feature (date-prefixed, slugified)
5. Parse Claude Code's structured output into PRD and TODO artifacts
6. Write `PRD.md`, `TODO.md`, and `.session` (session ID) to the output directory
7. On successful exit, output the path to `PRD.md` (goal-specific exit output)

The PRD must include a **Testing Plan** section with: test level (E2E/Integration/Unit), list of acceptance tests, target test file paths, and strong assertions.

### Acceptance-Tests Workflow

1. Read PRD.md and TODO.md from the plan directory specified by `--plan-dir`
2. Read the session ID from `.session` in the plan directory
3. Resume the Claude session using `--resume <session-id>`
4. Use `--permission-mode acceptEdits` (auto-approves file edits for creating tests and running `cargo test`)
5. **Q&A phase**: When Claude returns clarifying questions (e.g., permission requests), the user provides answers and the workflow continues
6. System prompt instructs Claude to: read the testing plan from the PRD; create acceptance tests as specified; verify all new tests fail (Red state); remove or adjust any tests that pass
7. Parse Claude's output to extract a summary of created tests and their status
8. Write `acceptance-tests.md` to the plan directory (structured list + rich descriptions for downstream goals)
9. On successful exit, output a human-readable summary (test count, paths, failing status)

### Red Workflow

1. Read PRD.md and acceptance-tests.md from the plan directory specified by `--plan-dir`
2. Start a fresh Claude session (does not resume; no `.session` required)
3. Use `--permission-mode acceptEdits` with same allowlist as acceptance-tests
4. System prompt instructs Claude to: plan implementation structure; create skeleton code that compiles; write failing lower-level tests; run `cargo test` to verify tests fail; emit structured response with tests and skeletons
5. Parse Claude's output to extract summary, tests, and skeletons
6. On successful exit, output summary, test list, and skeleton list

### LLM Backend Abstraction

1. The system defines a Rust trait (`CodingBackend` or similar) for LLM interactions
2. Claude Code CLI is the first concrete implementation
3. The trait must support: invoking the LLM, passing prompts, receiving structured output
4. The backend must support **model selection** (pass model name to the underlying CLI/API)
5. Tests use a mock implementation that allows test-controlled responses and behavior

### Claude Code Integration

1. Invokes `claude` CLI binary (from PATH)
2. **Print mode**: tddy-coder always uses `-p` (print mode) for non-interactive, single-query execution. In print mode, **stdin is not used for interactive permission prompts** — Claude Code handles permissions via `--permission-mode`, `--allowedTools`, or `--permission-prompt-tool`, not by reading user input from stdin.
3. **Plan goal**: Uses `--permission-mode plan` (read-only analysis) plus a predefined allowlist (`Read`, `Glob`, `Grep`, `SemanticSearch`) passed as `--allowedTools`.
4. **Acceptance-tests goal** and **Red goal**: Use `--permission-mode acceptEdits` plus a predefined allowlist (`Read`, `Write`, `Edit`, `Glob`, `Grep`, `Bash(cargo *)`, `SemanticSearch`) passed as `--allowedTools`.
5. **Hybrid permission policy**: Each goal has a built-in allowlist; tools matching the allowlist are auto-approved. Optional `--allowed-tools` CLI flag adds extra tools to the allowlist. Unexpected permission requests (not in the allowlist) are denied in non-interactive mode; interactive handling via embedded permission tool is available when enabled.
6. **Model selection**: Passes `--model <name>` to the `claude` binary when the user specifies one via `--model` / `-m`. Default model when unspecified (e.g. `opus` or backend default).
7. **Output format**: Uses `--output-format=stream-json` for NDJSON event stream (tool_use, result, task_progress).
8. **Session management**: First invoke uses `--session-id <uuid>`; Q&A followup uses `--resume <uuid>` so Claude retains context across the exchange.
9. **Structured Q&A**: Clarifying questions come from `AskUserQuestion` tool events (header, question, options, multi_select). Presented via inquire Select/MultiSelect prompts.
10. **Real-time progress**: Tool activity (Read, Glob, Bash, etc.) displayed while Claude works.
11. **Output parsing**: System prompt instructs Claude to emit PRD and TODO in `<structured-response content-type="application-json">` format; parser also supports delimiter fallback.

### Output Artifacts

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
- Rich descriptions for LLM consumption in subsequent goals (e.g. red)

#### red-output.md

- Written by the red goal to the plan directory
- **How to run tests**: Same as acceptance-tests.md
- **Prerequisite actions**: Same as acceptance-tests.md
- **How to run a single or selected tests**: Same as acceptance-tests.md
- Lists failing tests and skeletons (trait, struct, method, function, module)

#### progress.md

- Written by the red goal to the plan directory
- Unfilled milestones and checkboxes for failed tests and skeletons
- Next goal uses this document to mark items as done, skipped, or failed

### State Machine

1. The planning step is one state in the overall workflow state machine
2. **Plan goal**: Transitions `Init` → `Planning` → `Planned` (or `Failed`)
3. **Acceptance-tests goal**: Transitions `Init`/`Planned` → `AcceptanceTesting` → `AcceptanceTestsReady` (or `Failed`)
4. **Red goal**: Transitions `Init`/`Planned`/`AcceptanceTestsReady` → `RedTesting` → `RedTestsReady` (or `Failed`)
5. The state machine enforces that planning must complete before development begins
6. State transitions are explicit and auditable

### Exit Output

On successful completion, the program prints a goal-specific artifact path to stdout (one line):

- **plan**: Path to `PRD.md` (e.g. `./2026-03-07-feature-slug/PRD.md`)
- **acceptance-tests**: Summary of created tests and their failing status (requires `--plan-dir`)
- **red**: Summary of created skeletons and failing tests (requires `--plan-dir`)

This enables scripting and piping (e.g. `tddy-coder --goal plan < feature.txt | xargs cat`).

## Acceptance Criteria

### Plan Goal

- [ ] `tddy-coder --goal plan` reads from stdin and produces a named output directory
- [ ] Output directory contains well-formed `PRD.md`, `TODO.md`, and `.session`
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
- [ ] Error handling: missing plan-dir, missing PRD.md, missing .session, session resume failure
- [ ] `--model` and `--agent-output` flags work with the acceptance-tests goal
- [ ] acceptance-tests goal writes acceptance-tests.md to the plan directory

### Red Goal

- [ ] `tddy-coder --goal red --plan-dir <path>` creates skeleton code and failing lower-level tests
- [ ] Red goal reads PRD.md and acceptance-tests.md from plan directory
- [ ] Red goal starts fresh session (no resume)
- [ ] Red goal uses AcceptEdits permission mode and correct allowlist
- [ ] Output prints summary, test list, and skeleton list
- [ ] State machine transitions: Init/Planned/AcceptanceTestsReady → RedTesting → RedTestsReady
- [ ] Error handling: missing plan-dir, missing PRD.md, missing acceptance-tests.md
- [ ] `--model` and `--agent-output` flags work with the red goal

## Future Considerations (Not In Scope)

- Multi-turn refinement after initial plan (invoke → review → refine)
- Support for backends other than Claude Code
- File dependency analysis (Bazel-like)
- Test coverage and mutation testing integration
- Demo setup for user review
- Language-agnostic clean code analysis

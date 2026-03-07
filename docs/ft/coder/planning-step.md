# Planning Step — Feature Document

**Product Area**: Coder
**Status**: Draft
**Updated**: 2026-03-07 (goal-specific exit output)

## Summary

The Planning Step is the first phase of the tddy-coder workflow. It accepts a user's goal description via stdin, invokes an LLM backend (Claude Code CLI) in plan mode, and produces a structured planning output: a named directory containing a `PRD.md` (Product Requirements Document) and a `TODO.md` (implementation task list).

## Background

tddy-coder is a strict, state-machine-driven TDD workflow orchestrator. It uses LLM-based coders (starting with Claude Code) as backends to drive development from planning through production. The planning step is the entry point of the workflow — before any code is written, the system must produce a clear requirements document and a structured execution plan.

The tool treats the LLM as a subordinate: it instructs the LLM what to analyze, constrains its behavior via plan mode, and captures structured output. The LLM does not drive the workflow — the state machine does.

## Requirements

### CLI Interface

1. Binary name: `tddy-coder`
2. Accepts `--goal plan` to trigger the planning step
3. Accepts `--output-dir <path>` to configure where planning output is written (defaults to current directory)
4. Accepts `--model <name>` (or `-m <name>`) to select the LLM model (e.g. `opus`, `sonnet`, `haiku`)
5. Reads the feature description from stdin (supports piped input and interactive prompt)
6. *Deferred*: `--list-models` to list available models (not needed for current scope)

### Planning Workflow

1. Read feature description from stdin
2. Invoke Claude Code CLI in plan mode to analyze the feature description
3. **Q&A phase**: Claude Code may ask clarifying questions; the user is expected to answer them. The system must support this interactive exchange (Claude asks → user answers → Claude continues analysis).
4. Generate a deterministic directory name based on the feature (date-prefixed, slugified)
5. Parse Claude Code's structured output into PRD and TODO artifacts
6. Write `PRD.md` and `TODO.md` to the output directory
7. On successful exit, output the path to `PRD.md` (goal-specific exit output)

### LLM Backend Abstraction

1. The system defines a Rust trait (`CodingBackend` or similar) for LLM interactions
2. Claude Code CLI is the first concrete implementation
3. The trait must support: invoking the LLM, passing prompts, receiving structured output
4. The backend must support **model selection** (pass model name to the underlying CLI/API)
5. Tests use a mock implementation that allows test-controlled responses and behavior

### Claude Code Integration

1. Invokes `claude` CLI binary (from PATH)
2. Uses plan mode via `--permission-mode plan` (read-only analysis)
3. **Model selection**: Passes `--model <name>` to the `claude` binary when the user specifies one via `--model` / `-m`. Default model when unspecified (e.g. `opus` or backend default).
4. Supports **interactive Q&A**: Claude may ask clarifying questions during planning; the user provides answers. The invocation model must allow this exchange (e.g. multi-turn or interactive session).
5. Passes a system prompt instructing Claude to produce PRD and TODO content in a parseable format, or to output clarifying questions in `---QUESTIONS_START---` / `---QUESTIONS_END---` delimiters

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

### State Machine

1. The planning step is one state in the overall workflow state machine
2. It transitions from `Init` → `Planning` → `Planned` (or `Failed`)
3. The state machine enforces that planning must complete before development begins
4. State transitions are explicit and auditable

### Exit Output (Updated: 2026-03-07)

On successful completion, the program prints a goal-specific artifact path to stdout (one line):

- **plan**: Path to `PRD.md` (e.g. `./2026-03-07-feature-slug/PRD.md`)

This enables scripting and piping (e.g. `tddy-coder --goal plan < feature.txt | xargs cat`).

## Acceptance Criteria

- [ ] `tddy-coder --goal plan` reads from stdin and produces a named output directory
- [ ] Output directory contains well-formed `PRD.md` and `TODO.md`
- [ ] `--output-dir` flag controls output location
- [ ] `--model <name>` selects the LLM model; default used when omitted
- [ ] *Deferred*: `--list-models` lists available models
- [ ] Claude Code CLI is invoked in plan mode with appropriate arguments
- [ ] **Q&A support**: When Claude asks clarifying questions during planning, the user can provide answers and Claude continues analysis
- [ ] CodingBackend trait enables mock-based testing without real Claude Code CLI
- [ ] Tests use a fake/mock backend to verify the planning workflow end-to-end
- [ ] Error cases handled: empty input, Claude Code not found, malformed LLM output
- [ ] State machine enforces valid transitions
- [ ] On successful plan completion, stdout prints the path to `PRD.md` (goal-specific exit output)

## Future Considerations (Not In Scope)

- Multi-turn refinement after initial plan (invoke → review → refine)
- Support for backends other than Claude Code
- File dependency analysis (Bazel-like)
- Test coverage and mutation testing integration
- Demo setup for user review
- Language-agnostic clean code analysis

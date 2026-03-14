# Planning Step — Feature Document

**Product Area**: Coder
**Status**: Draft
**Updated**: 2026-03-07

## Summary

The Planning Step is the first phase of the tddy-coder workflow. When `--goal` is omitted, tddy-coder runs the full workflow (plan → acceptance-tests → red → green → demo-prompt → evaluate → validate → refactor → update-docs) in a single invocation, with auto-resume from `changeset.yaml` state. When a specific goal is given, it executes that step only. The **plan** goal accepts a user's goal description via stdin or `--prompt`, invokes an LLM backend (Claude or Cursor per `--agent`) in plan mode, and produces a structured planning output: a named directory containing a `PRD.md` (Product Requirements Document), `TODO.md` (implementation task list), and `changeset.yaml` (unified manifest with session ID, workflow state, discovery, and models). The **acceptance-tests** goal reads a completed plan from `changeset.yaml`, creates a fresh session (no plan resume), creates failing acceptance tests, writes `acceptance-tests.md`, and verifies they fail.

## Background

tddy-coder is a strict, state-machine-driven TDD workflow orchestrator. It uses LLM-based coders (starting with Claude Code) as backends to drive development from planning through production. The planning step is the entry point of the workflow — before any code is written, the system must produce a clear requirements document and a structured execution plan.

The tool treats the LLM as a subordinate: it instructs the LLM what to analyze, constrains its behavior via plan mode, and captures structured output. The LLM does not drive the workflow — the state machine does.

## Requirements

### CLI Interface (Updated: 2026-03-07)

1. Binary name: `tddy-coder`
2. When `--goal` is omitted, runs the full workflow (plan → acceptance-tests → red → green → demo-prompt → evaluate → validate → refactor → update-docs) with auto-resume from `changeset.yaml` state
3. Accepts `--goal plan` to trigger the planning step
4. Accepts `--goal acceptance-tests` to create failing acceptance tests from a completed plan
5. Accepts `--goal red`, `--goal green`, `--goal demo`, `--goal evaluate`, and `--goal update-docs` for the implementation and evaluation phases
6. Accepts `--allowed-tools <tools>` (comma-separated) to add extra tools to the goal's allowlist (e.g. `Bash(npm install)`)
7. Accepts `--plan-dir <path>`: path to plan output directory; required when `--goal acceptance-tests`, `--goal red`, `--goal green`, `--goal demo`, `--goal evaluate`, or `--goal update-docs`; used for resume when running full workflow
8. Planning output always goes to `$HOME/.tddy/sessions/{uuid}/` (stable session directory)
9. Accepts `--model <name>` (or `-m <name>`) to select the LLM model (e.g. `opus`, `sonnet`, `haiku`)
10. Accepts `--conversation-output <path>` to log the entire agent conversation in raw bytes to a file (Updated: 2026-03-07)
11. Accepts `--debug` to print CLI command and cwd before running (for debugging empty output)
12. Accepts `--agent <name>` to select backend: `claude` (default) or `cursor`
13. Reads the feature description from stdin (supports piped input and interactive prompt), or from `--prompt <text>` when provided
14. **TUI mode**: When both stdin and stderr are TTY, runs full TUI (activity log, inbox when Running, status bar, prompt bar). Agent output is always visible. During Running mode, users can queue prompts in the inbox; queued items are displayed, navigable (Up/Down), editable (E), and deletable (D); on workflow completion with non-empty inbox the first item is auto-dequeued and sent; when inbox is empty, mode transitions to FeatureInput so the user can start a new workflow without restarting. Piped/non-TTY uses plain linear output.
15. *Deferred*: `--list-models` to list available models (not needed for current scope)

### Planning Workflow (Updated: 2026-03-07)

1. Read feature description from `--prompt` (if set) or stdin (piped or interactive)
2. Invoke the selected backend (Claude or Cursor) in plan mode to analyze the feature description
3. **Q&A phase**: The agent may ask clarifying questions; the user is expected to answer them. The system must support this interactive exchange (Claude asks → user answers → Claude continues analysis).
4. Create output directory: `$HOME/.tddy/sessions/{uuid}/`
5. Structured output is received via `tddy-tools submit` (Unix socket IPC). Parser deserializes JSON into PRD, TODO, discovery, and demo plan artifacts.
6. `changeset.yaml` is created before the workflow starts (with initial_prompt, state.current = Init, empty sessions). The plan step updates it with PRD.md, TODO.md, discovery, models, clarification_qa. Session entries and `state.session_id` are written when the first stream event with session_id arrives.
7. **Plan approval gate**: After plan completes, the user is presented with three choices: View (full-screen PRD modal), Approve (proceed to acceptance-tests), or Refine (free-text feedback that resumes the LLM session). The approval loop continues until the user approves.
8. On successful exit, output the path to `PRD.md` (goal-specific exit output)

The PRD must include a **Testing Plan** section with: test level (E2E/Integration/Unit), list of acceptance tests, target test file paths, and strong assertions.

### Plan Approval Gate (Updated: 2026-03-10)

After the plan step produces PRD.md and TODO.md, the workflow presents an approval gate before proceeding to acceptance-tests:

- **View**: Full-screen tui-markdown modal showing PRD.md. Keyboard scrolling (Up/Down, PageUp/PageDown). Q or Esc dismisses and returns to the approval menu.
- **Approve**: Proceeds to acceptance-tests.
- **Refine**: Text input mode for feedback. The workflow resumes the plan session with the feedback, re-runs plan, re-writes artifacts, and re-presents the approval gate.

In plain mode (non-TTY): text prompt `[v] View  [a] Approve  [r] Refine`; read user choice from stdin.

The approval gate applies to both the initial plan and plan resume/completion scenarios.

**Orchestration**: Elicitation is hook-triggered. `TddWorkflowHooks` signals `PlanApproval` after the plan task when PRD.md exists; the orchestrator returns `ElicitationNeeded` to the caller instead of auto-continuing. The caller presents the approval UI, collects the user's choice, and resumes the workflow.

**Dependencies**: `tui-markdown` crate in tddy-tui for markdown rendering.

### Acceptance-Tests Workflow

1. Read PRD.md and TODO.md from the plan directory specified by `--plan-dir`
2. Read the model from `changeset.yaml` in the plan directory
3. Create a fresh Claude session (does not resume the plan session)
4. Use `--permission-mode acceptEdits` (auto-approves file edits for creating tests and running `cargo test`)
5. **Q&A phase**: When Claude returns clarifying questions (e.g., permission requests), the user provides answers and the workflow continues
6. System prompt instructs Claude to: read the testing plan from the PRD; create acceptance tests as specified; verify all new tests fail (Red state); remove or adjust any tests that pass
7. Parser receives JSON from `tddy-tools submit`; deserializes summary of created tests and their status
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
8. **Session management**: First invoke uses `--session-id <uuid>`; Q&A followup uses `--resume <uuid>` so Claude retains context across the exchange. Session IDs are persisted in `changeset.yaml`. The `state.session_id` field is the single source of truth for the currently-active agent session; session entries and `state.session_id` are written when the first stream event with session_id arrives (not after the step completes).
9. **Structured Q&A**: Clarifying questions come from `AskUserQuestion` tool events (header, question, options, multi_select). In TUI mode, presented via ratatui Select/MultiSelect widgets with "Other (type your own)" option. In plain mode, presented via stdin (one answer per line). Questions and answers are stored in `changeset.yaml` as `clarification_qa`.
10. **Real-time progress**: Tool activity (Read, Glob, Bash, etc.) displayed while Claude works.
11. **Structured output**: System prompt instructs the agent to call `tddy-tools submit --goal <goal> --data '<json>'`. All structured output is received via Unix socket IPC; the parser deserializes pre-validated JSON. No inline parsing (XML blocks or delimiters). If the agent finishes without calling `tddy-tools submit`, the workflow fails with a clear diagnostic.
12. **Schema validation**: `tddy-tools submit --goal <goal>` validates output against embedded JSON schemas in tddy-tools. No schema files are written to disk. Agents run `tddy-tools get-schema <goal>` to inspect the expected format. On validation failure, tddy-tools returns errors with a tip to run `tddy-tools get-schema <goal>`. (Updated: 2026-03-12)

### Project Discovery (Plan Goal)

The plan goal performs read-only discovery before producing the PRD:
- Parse `Cargo.toml`, `package.json`, `Makefile`, `flake.nix`, `.nvmrc`, `.tool-versions`, `.python-version`, `AGENTS.md` for tool/SDK versions and scripts
- Identify documentation locations (`docs/`, `packages/*/docs/`, README files)
- Discover the best location for the plan directory based on repo conventions (`plan_dir_suggestion` in discovery)
- Reveal relevant code areas (modules, traits, key files)
- Capture test infrastructure (runners, conventions, CI scripts)

Discovery is persisted in `changeset.yaml` for downstream goals.

### Plan Directory Relocation

When the agent returns `plan_dir_suggestion` in discovery, the workflow relocates the plan directory from its staging location (e.g. `output_dir/2026-03-08-feature/`) to the suggested path relative to the git root (e.g. `git_root/docs/dev/1-WIP/2026-03-08-feature/`). The agent runs in the staging directory first (with schemas available); after artifacts are written, the directory is moved. Invalid suggestions (absolute paths, `..`, empty) fall back to the staging location. Cross-device moves use copy-then-delete when rename fails.

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
- `branch_suggestion`: Git branch name suggested by the plan agent (e.g. "feature/auth"). Used for worktree creation after plan approval.
- `worktree_suggestion`: Worktree directory name suggested by the plan agent (e.g. "feature-auth"). Used for `git worktree add` after plan approval.
- `initial_prompt`: User's goal/feature description from stdin
- `clarification_qa`: Questions asked during planning and user's answers (empty if no clarification)
- `sessions`: Array of session entries (id, agent, tag, created_at, system_prompt_file)
- `state`: Current workflow state, `session_id` (currently-active agent session), and history
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
3. **Plan approval gate**: Between `Planned` and `AcceptanceTesting`, the user must approve (View/Approve/Refine). No new changeset states; the gate is a presenter/workflow-runner concern.
4. **Acceptance-tests goal**: Transitions `Init`/`Planned` → `AcceptanceTesting` → `AcceptanceTestsReady` (or `Failed`)
5. The state machine enforces that planning must complete before development begins
6. State transitions are explicit and auditable

### Exit Output

On successful completion, the program prints goal-specific output to stdout:

- **Full workflow** (no `--goal`): Green step output (summary, tests, implementations); prints plan dir path at end
- **plan**: Plan directory path (e.g. `./2026-03-07-feature-slug/` or relocated path)
- **acceptance-tests**: Summary of created tests and their failing status; prints plan dir path (requires `--plan-dir`)
- **red**, **green**: Summary of created tests/skeletons or implementations; prints plan dir path

This enables scripting and piping (e.g. `tddy-coder --goal plan < feature.txt` outputs the plan dir path).

### Full Workflow (No --goal)

When `--goal` is omitted, tddy-coder runs plan → acceptance-tests → red → green → demo-prompt → evaluate → validate → refactor → update-docs in sequence. After green completes, the user is prompted "Run demo? [r] Run [s] Skip"; Run executes the demo step, Skip proceeds directly to evaluate. Resume requires `--plan-dir`: if interrupted, re-run with `--plan-dir <path>` to skip completed steps (reads `changeset.yaml.state.current`). Without `--plan-dir`, a new plan is started. When state is `Evaluated`, re-running exits with a summary. `changeset.yaml` is written immediately after the user enters their prompt (before the plan agent runs), so the plan dir is resumable even if planning fails.

## Acceptance Criteria

### Full Workflow (No --goal)

- [x] `tddy-coder` (no `--goal`) reads from stdin and runs plan → acceptance-tests → red → green → demo-prompt → evaluate
- [x] After green completes, user is prompted to run or skip demo; Skip proceeds to evaluate
- [x] Full workflow prints evaluate step output on success
- [x] Full workflow supports resume via `--plan-dir` (required; no auto-detect)
- [x] When state is Evaluated, re-running exits with summary
- [x] `changeset.yaml` exists on disk immediately after user enters prompt, before plan agent runs

### Plan Goal

- [x] `tddy-coder --goal plan` reads from stdin and produces a named output directory
- [ ] Output directory contains well-formed `PRD.md`, `TODO.md`, and `changeset.yaml`
- [x] Output location fixed at `$HOME/.tddy/sessions/{uuid}/`
- [ ] `--model <name>` selects the LLM model; default used when omitted
- [ ] *Deferred*: `--list-models` lists available models
- [ ] Claude Code CLI is invoked in plan mode with appropriate arguments
- [ ] **Q&A support**: When Claude asks clarifying questions during planning, the user can provide answers and Claude continues analysis
- [ ] Plan system prompt produces a Testing Plan section in the PRD (test level, acceptance tests list, target files, assertions)
- [ ] CodingBackend trait enables mock-based testing without real Claude Code CLI
- [ ] Tests use a fake/mock backend to verify the planning workflow end-to-end
- [ ] Error cases handled: empty input, Claude Code not found, malformed LLM output
- [ ] State machine enforces valid transitions
- [ ] On successful plan completion, stdout prints the plan directory path (goal-specific exit output)

### Acceptance-Tests Goal

- [ ] `tddy-coder --goal acceptance-tests --plan-dir <path>` creates failing acceptance tests from a plan
- [ ] Acceptance-tests goal creates a fresh session (does not resume the plan session)
- [ ] Claude runs tests and verifies all new tests fail (Red state); passing tests are adjusted or removed
- [ ] Output prints a summary of created tests, their paths, and failing status
- [ ] State machine transitions: Init/Planned → AcceptanceTesting → AcceptanceTestsReady
- [ ] Error handling: missing plan-dir, missing PRD.md, missing changeset.yaml, session resume failure
- [ ] `--model` flag works with the acceptance-tests goal
- [ ] acceptance-tests goal writes acceptance-tests.md to the plan directory

## Future Considerations (Not In Scope)

- Multi-turn refinement after initial plan (invoke → review → refine)
- File dependency analysis (Bazel-like)
- Test coverage and mutation testing integration
- Demo setup for user review
- Language-agnostic clean code analysis

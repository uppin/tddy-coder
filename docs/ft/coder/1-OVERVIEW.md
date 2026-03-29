# Coder — Product Area Overview

**Type**: Technical Product (Developer Tool)
**Status**: Active
**Updated**: 2026-03-29

## Summary

tddy-coder is a TDD-driven development CLI that orchestrates an LLM coding backend (Claude Code CLI, Claude ACP, Cursor agent, OpenAI Codex CLI, or Stub) through a strict workflow: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs. It produces structured artifacts (PRD.md, TODO.md, acceptance-tests.md, progress.md, etc.) in a plan directory and maintains workflow state in changeset.yaml. The tool supports both TUI mode (interactive ratatui interface) and plain mode (linear output for piping and scripting).

## Target Users

- **Developers** using TDD to build features from a natural-language description
- **Teams** adopting structured planning and acceptance-test-driven workflows
- **Automation** via piping, `--prompt`, and gRPC remote control

## Core Capabilities

| Capability | Description |
|------------|--------------|
| **Planning** | Accepts feature description via stdin or `--prompt`; invokes LLM in plan mode; produces PRD.md, TODO.md, changeset.yaml |
| **Plan Approval** | After plan completes, user can View PRD, Approve (proceed), or Refine (feedback loop) |
| **Acceptance Tests** | Creates failing acceptance tests from PRD; writes acceptance-tests.md |
| **Red-Green** | Red creates skeletons and failing tests; Green implements production code to make them pass |
| **Demo** | Executes demo plan from demo-plan.md (optional, prompted after green) |
| **Evaluate** | Analyzes git changes for risks; produces evaluation-report.md |
| **Validate** | Subagent-driven validation (tests, prod-ready, clean code) |
| **Refactor** | Executes refactoring plan from validate phase |
| **Update Docs** | Reads planning artifacts and updates target repo documentation per repo guidelines |
| **TUI** | Full ratatui interface: activity log, status bar, inbox, clarification prompts, plan approval |
| **gRPC** | `--grpc` exposes bidirectional streaming for programmatic control (E2E tests, automation); `StreamTerminal` streams raw TUI bytes for remote viewing |
| **LiveKit** | `--livekit-url`, `--livekit-room`, `--livekit-identity` with either `--livekit-token` or `--livekit-api-key`/`--livekit-api-secret`. Key/secret generate tokens locally and auto-refresh before expiry. |
| **Web Bundle** | `--web-port` and `--web-bundle-path` serve pre-built tddy-web static assets over HTTP (TUI and daemon modes) |
| **Backend selection** | With `--agent` omitted, users pick the coding backend (Claude, Claude ACP, Cursor, Codex, Stub) via TUI `AppMode::Select` or a plain numbered menu. With `--agent` set, selection is skipped. Per-backend default models apply; `--model` overrides. Cursor receives `--model` on `cursor agent` when configured; Codex receives `-m` on `codex exec` when configured. |
| **Workflow recipe** | **`--recipe tdd`** (default) runs **`TddRecipe`**; **`--recipe bugfix`** runs **`BugfixRecipe`** (reproduce-first, fix-plan approval before green). Optional YAML **`recipe:`**; **`changeset.yaml`** stores **`recipe`** for resume. |
| **Project agent skills** | Skills under **`.agents/skills/<name>/SKILL.md`** with matching frontmatter **`name`**; **`tddy_core::agent_skills`** supplies discovery, slash menu items, and composed prompts; built-in **`/recipe`** in the presenter selects TDD vs Bugfix when wired with **`with_recipe_resolver`**. Slash completion in the ratatui feature input is outside this surface. |

## Backend selection at session start

- **CLI**: `--agent` is optional. Omitted → interactive choice before FeatureInput; set → that backend is used without a selection step.
- **TUI**: Synthetic clarification `Select` over backend options; `AppMode::Select` includes `initial_selected` for highlight consistency.
- **Plain**: Numbered menu on stderr; stdin line picks the backend when `--agent` is omitted.
- **Daemon / web**: `StartSession` includes `agent`; the daemon passes `--agent` into the spawned `tddy-coder`. The web Connection Screen offers backend per **new session** only (`StartSessionRequest.agent`). The choice is **per session**, not stored on the project record.
- **Models**: Defaults per backend (e.g. Cursor `composer-2`, Codex `gpt-5`); global `--model` overrides when provided.
- **Codex binary**: `--codex-cli-path` or environment variable `TDDY_CODEX_CLI` selects the `codex` executable; otherwise the `codex` name on `PATH` is used.

## Feature Documents

| Feature | Description |
|---------|-------------|
| [Workflow JSON Schemas](workflow-json-schemas.md) | JSON Schema contracts per goal; `goals.json` registry; `tddy-tools` `get-schema`, `list-schemas`, `submit` validation |
| [Workflow recipes](workflow-recipes.md) | Pluggable `WorkflowRecipe`; `TddRecipe` (default) and `BugfixRecipe` (selectable); `recipe_resolve` in `tddy-workflow-recipes`; `GoalId` / string states |
| [Planning Step](planning-step.md) | Plan goal, acceptance-tests goal, plan approval gate, CLI interface, LLM backend abstraction |
| [Implementation Step](implementation-step.md) | Red, green, demo, evaluate goals; state machine; output artifacts |
| [gRPC Remote Control](grpc-remote-control.md) | `--grpc` flag, bidirectional streaming, programmatic control for E2E and automation |
| [TUI status bar](tui-status-bar.md) | Spinner and session segment on the status line; parity with Virtual TUI / streamed frames |
| [Feature prompt: agent skills](feature-prompt-agent-skills.md) | **`.agents/skills`** discovery, composed skill prompts, presenter **`/recipe`** selection |

## Integration Points

- **tddy-core**: Workflow engine (`WorkflowEngine`), recipe trait (`WorkflowRecipe`), graph execution, `RunnerHooks`, `CodingBackend` trait; goals and states are string IDs, not a fixed enum
- **tddy-workflow-recipes**: `TddRecipe`, `BugfixRecipe`, and **`recipe_resolve`** (single source for CLI/daemon recipe names); graph definitions, hooks, parsers, and backend hints per recipe
- **tddy-tui**: Ratatui view layer, PresenterView implementation, key mapping
- **tddy-service**: gRPC service, proto definitions, event conversion (renamed from tddy-grpc; contains EchoServiceImpl, TerminalServiceImpl, DaemonService)
- **tddy-demo**: Same app with StubBackend for demos and E2E tests
- **tddy-livekit-web**: ConnectRPC Transport over LiveKit data channels for browser clients calling Rust RPC services
- **Claude Code CLI / Cursor / Codex**: LLM backends invoked via subprocess; Codex uses `codex exec` JSONL (`--json`)

## Change History

See [changelog.md](changelog.md) for release note history.

## Appendices

Technical specifications and supporting documentation:

- **[Technology Stack](../../dev/guides/tech-stack.md)** — Core technologies, integration patterns
- **[Testing Practices](../../dev/guides/testing.md)** — Anti-patterns, unit/integration/production test guidelines

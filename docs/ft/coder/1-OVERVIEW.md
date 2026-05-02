# Coder — Product Area Overview

**Type**: Technical Product (Developer Tool)
**Status**: Active
**Updated**: 2026-05-02

## Summary

tddy-coder is a TDD-driven development CLI that orchestrates an LLM coding backend (Claude Code CLI, Claude ACP, Cursor agent, OpenAI Codex CLI, OpenAI Codex via ACP (`codex-acp`), or Stub) through a strict workflow: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor → update-docs. It produces structured artifacts (PRD.md, TODO.md, acceptance-tests.md, progress.md, etc.) in a plan directory and maintains workflow state in changeset.yaml. The **`workflow`** subsection of **`changeset.yaml`** persists post-green routing, branch/worktree intent, and (**when present**) optional post-workflow GitHub PR and session-worktree elicitation fields validated by **`changeset-workflow`**; see **[post-workflow-github-pr-elicitation.md](post-workflow-github-pr-elicitation.md)**. The tool supports both TUI mode (interactive ratatui interface) and plain mode (linear output for piping and scripting).

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
| **Backend selection** | With `--agent` omitted, users pick the coding backend (Claude, Claude ACP, Cursor, Codex, Codex ACP, Stub) via TUI `AppMode::Select` or a plain numbered menu. With `--agent` set, selection is skipped. Per-backend default models apply; `--model` overrides. Cursor receives `--model` on `cursor agent` when configured; Codex receives `-m` on `codex exec` when configured. |
| **Workflow recipe** | **`--recipe`** selects **`TddRecipe`**, **`TddSmallRecipe`**, **`BugfixRecipe`**, **`FreePromptingRecipe`**, or **`GrillMeRecipe`** (**`tdd`**, **`tdd-small`**, **`bugfix`**, **`free-prompting`**, **`grill-me`**). **New sessions** with no **`--recipe`** and no **`recipe`** in **`changeset.yaml`** use **`free-prompting`**. Optional YAML **`recipe:`**; **`changeset.yaml`** stores **`recipe`** for resume. In TUI **FeatureInput**, **`/start-<recipe>`** lines switch recipe and restart the workflow; after a structured **`/start-*`** run completes, the session returns to **`free-prompting`**. |
| **Project agent skills** | Skills under **`.agents/skills/<name>/SKILL.md`** with matching frontmatter **`name`**; **`tddy_core::agent_skills`** supplies discovery, slash menu items (**`/start-…`**, **`/recipe`**, skills), and composed prompts; built-in **`/recipe`** in the presenter opens recipe selection when wired with **`with_recipe_resolver`**. Slash completion in the ratatui feature input is outside this surface. |

## Backend selection at session start

- **CLI**: `--agent` is optional. Omitted → interactive choice before FeatureInput; set → that backend is used without a selection step.
- **TUI**: Synthetic clarification `Select` over backend options; `AppMode::Select` includes `initial_selected` for highlight consistency.
- **Plain**: Numbered menu on stderr; stdin line picks the backend when `--agent` is omitted.
- **Daemon / web**: `StartSession` includes `agent`; the daemon passes `--agent` into the spawned `tddy-coder`. The web Connection Screen offers backend per **new session** only (`StartSessionRequest.agent`). The choice is **per session**, not stored on the project record.
- **Models**: Defaults per backend (e.g. Cursor `composer-2`, Codex / Codex ACP `gpt-5`); global `--model` overrides when provided.
- **Codex binary**: `--codex-cli-path` or environment variable `TDDY_CODEX_CLI` selects the `codex` executable; otherwise the `codex` name on `PATH` is used.
- **Codex ACP binary**: `--codex-acp-cli-path`, `TDDY_CODEX_ACP_CLI`, optional YAML `codex_acp_cli_path`, or `codex-acp` on `PATH` when using `--agent codex-acp`. See [codex-acp-backend.md](codex-acp-backend.md).

## Feature Documents

| Feature | Description |
|---------|-------------|
| [Session actions](session-actions.md) | Declarative **`actions/*.yaml`** beside **`changeset.yaml`**; **`tddy-tools list-actions`** / **`invoke-action`**; JSON Schema inputs; optional cargo-style summaries; path sandbox aligned with **`repo_path`** |
| [Workflow JSON Schemas](workflow-json-schemas.md) | JSON Schema contracts per goal; `goals.json` registry; `tddy-tools` `get-schema`, `list-schemas`, `submit` validation |
| [Post-workflow GitHub PR and worktree elicitation](post-workflow-github-pr-elicitation.md) | Durable **`changeset.workflow`** post-workflow fields; **`persist-changeset-workflow`**; **`merge_persisted_workflow_into_context`**; **`post_workflow`** helpers for ordering, resume gating, and operator-facing PR status strings |
| [GitHub pull request tools (MCP)](github-pr-tools-mcp.md) | **`github_create_pull_request`** / **`github_update_pull_request`** on **`tddy-tools --mcp`**; shared REST constants; merge-pr and **tdd-small** prompt gating |
| [Workflow recipes](workflow-recipes.md) | Pluggable `WorkflowRecipe`; shipped recipes include **`TddRecipe`**, **`TddSmallRecipe`**, **`BugfixRecipe`**, **`FreePromptingRecipe`**, and **`GrillMeRecipe`**; **new sessions** default to **`free-prompting`** when no recipe is specified; `recipe_resolve` in `tddy-workflow-recipes`; `GoalId` / string states; **FeatureInput** **`/start-<recipe>`** and slash menu rows. **Grill me** **Create plan** brief: session `artifacts/grill-me-brief.md`; repo persistence per [AGENTS.md](../../../AGENTS.md) (`plans/` or feature-doc path). (Updated: 2026-04-05) |
| [Planning Step](planning-step.md) | Plan goal, acceptance-tests goal, plan approval gate, CLI interface, LLM backend abstraction |
| [Implementation Step](implementation-step.md) | Red, green, demo, evaluate goals; state machine; output artifacts |
| [gRPC Remote Control](grpc-remote-control.md) | `--grpc` flag, bidirectional streaming, programmatic control for E2E and automation |
| [TUI status bar](tui-status-bar.md) | Spinner and session segment on the status line; parity with Virtual TUI / streamed frames |
| [Feature prompt: agent skills](feature-prompt-agent-skills.md) | **`.agents/skills`** discovery, composed skill prompts, presenter **`/recipe`** selection |
| [Activity log streaming](activity-log-streaming.md) | User **`User:`** / **`Queued:`** lines in the activity log; incremental agent tail; **`AgentOutput`** as the streaming channel for workflow chunks |
| [Codex ACP backend](codex-acp-backend.md) | **`--agent codex-acp`**: ACP to **`codex-acp`** subprocess; resume via **`load_session`**; **`codex_thread_id`** parity with **`codex`**; OAuth retry via **`codex login`** and **`codex_oauth_authorize.url`** |

## Integration Points

- **tddy-core**: Workflow engine (`WorkflowEngine`), recipe trait (`WorkflowRecipe`), graph execution, `RunnerHooks`, `CodingBackend` trait; goals and states are string IDs, not a fixed enum
- **tddy-workflow-recipes**: `TddRecipe`, `BugfixRecipe`, and **`recipe_resolve`** (single source for CLI/daemon recipe names); graph definitions, hooks, parsers, and backend hints per recipe
- **tddy-tui**: Ratatui view layer, PresenterView implementation, key mapping
- **tddy-service**: gRPC service, proto definitions, event conversion (renamed from tddy-grpc; contains EchoServiceImpl, TerminalServiceImpl, DaemonService)
- **tddy-demo**: Same app with StubBackend for demos and E2E tests
- **tddy-livekit-web**: ConnectRPC Transport over LiveKit data channels for browser clients calling Rust RPC services
- **Claude Code CLI / Cursor / Codex / Codex ACP**: LLM backends invoked via subprocess; Codex uses `codex exec` JSONL (`--json`); Codex ACP uses the **`codex-acp`** stdio agent and **`agent-client-protocol`** (see [codex-acp-backend.md](codex-acp-backend.md))

## Change History

See [changelog.md](changelog.md) for release note history.

## Appendices

Technical specifications and supporting documentation:

- **[Technology Stack](../../dev/guides/tech-stack.md)** — Core technologies, integration patterns
- **[Testing Practices](../../dev/guides/testing.md)** — Anti-patterns, unit/integration/production test guidelines

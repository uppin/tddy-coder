# Managed Codebase Workflow (workflow-aware, self-managing Claude CLI)

**Product area:** Coder
**Updated:** 2026-07-03

## Summary

A new-session **"Managed codebase"** option makes a Claude-CLI session *workflow-aware and
self-managing*. When enabled, the user picks a **workflow recipe** (`tdd`, `bugfix`, …) and,
optionally, one or more **specialized subagents**. The daemon then launches Claude so that it:

1. Receives the recipe's **orchestration system prompt** (`WorkflowRecipe::orchestration_system_prompt`),
   which describes the state machine, the `transition` tool contract, and the subagent go/no-go protocol.
2. Drives its own workflow by calling `tddy-tools transition --to <goal>`, which advances a
   **per-session** [`WorkflowController`] that validates the edge against the recipe graph and
   **persists** the new state into that session's `changeset.yaml`.

Previously these two concerns were split and neither reached a Claude-CLI session: recipe selection
was offered only for the `"tool"` session type (which spawns `tddy-coder`), and the "Managed codebase"
section on Claude-CLI sessions only attached specialized subagents (with no workflow). The agent-driven
machinery existed (`presenter/agent_session_runner.rs`, gated behind `TDDY_AGENT_DRIVEN`) but was wired
only into the in-process `tddy-coder` path. This feature exposes it to Claude-CLI sessions through an
explicit UI control, for both **sandboxed** and **non-sandboxed** sessions.

> **Terminology.** "Managed codebase" here means *workflow-managed* (Claude manages the workflow state
> machine). This is distinct from the remote managed-**filesystem** mode in
> [managed-codebase-subagents.md](managed-codebase-subagents.md), where the repo is not mounted and the
> agent reaches it only through `mcp__tddy-tools__*` exec tools. The two can co-exist on a sandboxed
> session; this document is about the workflow dimension.

## How the transition reaches the daemon

`tddy-tools transition` always runs **on the host**, so the existing `TDDY_SOCKET` relay is reused
unchanged (no `tddy-tools` change, no proto change — `StartSessionRequest.recipe` and
`.managed_codebase` already exist):

- **Sandboxed** sessions never mount the repo; the agent's `Shell` tool relays back to the daemon and
  runs `sh -c "<command>"` on the host in the worktree, so the `tddy-tools transition` child inherits
  the daemon-provided per-session env.
- **Non-sandboxed** sessions spawn `claude` in a host PTY; any `tddy-tools` child inherits the PTY env.

The single blocker was that the transition-handler registry (`toolcall/transition.rs`) is
process-global — unsafe for a daemon serving concurrent sessions. It is made **per-instance** on
`ToolcallRpcService`, and the daemon hosts **one toolcall listener per session** whose handler is that
session's `WorkflowController`. The listener socket path is injected as `TDDY_SOCKET` into the
per-session env (alongside a `PATH` that resolves `tddy-tools`).

## Architecture

```
Web CreateSessionPane (claude-cli)
  [x] Managed codebase                    StartSessionRequest
      Recipe:    [ tdd ▾ ]        ──────►   recipe = "tdd"
      Subagents: [x] fastcontext           managed_codebase = true
                                           specialized_agents = ["fastcontext"]
        │
        ▼
  daemon start_session → start_(sandboxed_)claude_cli_session(managed_recipe)
        │  seed changeset.yaml state = recipe.start_goal()
        │  set_up_managed_workflow: WorkflowController + per-session toolcall listener
        │  launch claude with --append-system-prompt-file <orchestration prompt>
        │  inject per-session env: TDDY_SOCKET=<listener>, PATH=<tddy-tools dir>:…
        ▼
  Claude runs → `tddy-tools transition --to <goal>` (on host)
        │  TDDY_SOCKET relay → per-session ToolcallRpcService → WorkflowController
        ▼
  WorkflowController validates edge, persists state.current → changeset.yaml
```

## User story

As a developer, I want to start a Claude-CLI session that follows a chosen workflow (e.g. TDD) and
manages its own progress, so that Claude advances through the workflow's goals and records its state in
`changeset.yaml` — the same durable state the rest of the system reads — instead of running as an
unstructured free-form session.

## Acceptance criteria

### Web (CreateSessionPane)
1. For `session_type == "claude-cli"`, a **"Managed codebase"** control is an explicit checkbox
   (`create-session-managed-codebase-toggle`). It is absent for the `"tool"` session type.
2. When the checkbox is enabled, the form reveals **both** a **workflow-recipe** picker
   (`create-session-recipe-select`) and the **specialized-subagents** multi-select
   (`create-session-managed-codebase-section`).
3. Submitting a managed Claude-CLI session sends `managed_codebase = true` and the selected `recipe`
   on `StartSessionRequest`; `managed_codebase` is the explicit flag (no longer implied by the number
   of selected subagents).
4. A managed session with a recipe and **no** subagents still sends `managed_codebase = true` and the
   selected `recipe` (a case the implied model could not express).
5. When the checkbox is disabled, the request carries `managed_codebase = false` and an empty `recipe`.

### Daemon (StartSession → claude-cli)
6. A managed Claude-CLI session (`managed_codebase = true`, non-empty `recipe`) seeds the session's
   `changeset.yaml` `state.current` with the recipe's start goal before launch (e.g. `interview` for
   `tdd`).
7. A managed Claude-CLI session with an **unknown** recipe is rejected with `INVALID_ARGUMENT`.
8. A managed Claude-CLI session launches `claude` with `--append-system-prompt-file` pointing at a file
   whose content equals `recipe.orchestration_system_prompt(start_goal)`.
9. A managed Claude-CLI session launches `claude` with a per-session `TDDY_SOCKET` (the session's
   toolcall listener) and a `PATH` that resolves `tddy-tools` in its environment.
10. An **unmanaged** Claude-CLI session (`managed_codebase = false`) launches with no orchestration
    prompt and no workflow wiring (behavior unchanged from today).
11. A host-side `transition` for a managed session validates against the recipe graph and persists the
    new `state.current` to `changeset.yaml`; an illegal transition is rejected and leaves the state
    unchanged.
12. Applies to **both** sandboxed and non-sandboxed Claude-CLI sessions. The explicit
    `managed_codebase` + `recipe` request drives it; `TDDY_AGENT_DRIVEN` remains only for the
    `tddy-coder` path and is unaffected.

### Concurrency
13. The transition handler is per-session (per-instance on `ToolcallRpcService`), so concurrent managed
    sessions never route a `transition` to another session's controller. The process-global registry
    remains as a fallback only for the in-process `tddy-coder`/`agent_session_runner` path.

## Non-goals (v1)
- Client-push streaming of workflow events for Claude-CLI managed sessions — state is persisted to
  `changeset.yaml` and reflected in `SessionMetadata.activity_status`; live event streaming to the web
  UI is a follow-up.
- `tddy-tools ask` / `approve` over the per-session toolcall listener — Claude-CLI sessions use the MCP
  `approval_prompt` tool + PTY for those; the per-session listener serves `transition` only.
- Managed workflow for the `"tool"` session type (it already has recipe selection via `tddy-coder`).
- Toggling sandbox mount behavior based on `managed_codebase` (the sandboxed path is already
  proxied-tools-only; see [managed-codebase-subagents.md](managed-codebase-subagents.md)).

## Related
- [Specialized subagents (picker)](specialized-subagents.md) — the multi-select this feature reuses.
- [Workflow recipes](workflow-recipes.md) — the recipe registry and `orchestration_system_prompt`.
- [Managed codebase (remote filesystem mode)](managed-codebase-subagents.md) — the other "managed" axis.

# PRD: Cursor CLI Feature Parity with Claude CLI

> **Status:** In Planning (2026-07-06)
> **Supersedes the "Out of scope (v1)" section of:** [cursor-cli-session.md](../daemon/cursor-cli-session.md)

## Summary

Bring the Cursor Agent CLI (`agent` binary) to **full feature parity** with the Claude Code CLI integration across both integration layers:

1. **Daemon interactive sessions** (`session_type = "cursor-cli"`) — sandbox mode, managed codebase workflow, specialized subagents, `WaitingForInput` activity.
2. **Workflow `CodingBackend` invoke path** (`CursorBackend::invoke`) — MCP + `permission-prompt-tool` headless approvals, `RemoteToolEnv` (`TDDY_REMOTE_*`).

This closes every gap listed in the v1 "Out of scope" of `cursor-cli-session.md`, plus the workflow-backend gaps between `ClaudeCodeBackend` and `CursorBackend`.

## Background and motivation

Cursor CLI support landed on 2026-07-05 as a v1 covering interactive PTY sessions and `CodingBackend` invocations. Claude CLI, by contrast, has a substantial production surface hardened over many iterations: macOS Seatbelt / Linux cgroups+namespace sandboxing, a managed-codebase workflow with orchestration prompts and `TDDY_SOCKET`, specialized subagents via `TDDY_SUBAGENTS_JSON`, headless tool approvals through MCP + `permission-prompt-tool`, remote-codebase env, and a richer activity-status mapping (`WaitingForInput`).

Users who run Cursor as their agent should not be forced back to Claude to get sandbox isolation, managed workflows, or unattended headless runs. The v1 explicitly rejected `sandbox=true` for cursor-cli with `FAILED_PRECONDITION` — this PRD removes that rejection and fills in the rest.

## Affected feature documents

This PRD amends/extends:

- `docs/ft/daemon/cursor-cli-session.md` — sandbox mode, `WaitingForInput`, managed codebase, specialized agents (re-opens v1 out-of-scope items).
- `docs/ft/daemon/claude-cli-session.md` — sibling reference; sandbox/managed/agents sections become shared concepts.
- `docs/ft/coder/managed-codebase-workflow.md` — extend to cursor-cli.
- `docs/ft/coder/managed-codebase-subagents.md` — extend to cursor-cli.
- `docs/ft/coder/specialized-subagents.md` — extend to cursor-cli.
- `docs/ft/daemon/remote-codebase-mode.md` — extend to cursor-cli sandbox.
- `packages/tddy-sandbox-recipes/docs/*` — new `cursor_cli` recipe.
- `packages/tddy-sandbox-app/` — gain `--agent cursor` mode for standalone sandbox proof (used by the acceptance proof step).

## High-level requirements

### R1 — Sandbox mode for cursor-cli (daemon + standalone app)

- **macOS Seatbelt:** new `cursor_cli` sandbox recipe in `tddy-sandbox-recipes` (parallel to `claude_cli.rs`): read grants, PTY/fork/mach/sysctl policy tuned for the `agent` binary (Node/V8 like Claude), credential seeding into jail home, MCP argv/env overlays for in-jail `tddy-tools --mcp`.
- **Linux cgroups + user namespaces:** parallel recipe in `tddy-sandbox-cgroups` (or shared recipe module) so cursor-cli sandbox works on Linux hosts, mirroring the claude-cli Linux path.
- **`connection_service.rs`:** add `start_sandboxed_cursor_cli_session` mirroring `start_sandboxed_claude_cli_session`. Remove the `FAILED_PRECONDITION` rejection for `session_type == "cursor-cli" && sandbox`.
- **`tddy-sandbox-app`:** extend `SpawnParams`/`spawn` to accept an agent kind (`claude` | `cursor`) so the standalone app can spawn a sandboxed `agent` (the proof vehicle). CLI flag e.g. `--agent cursor` + `--agent-binary`/`cursor_binary`.
- **Egress tunnel:** Cursor API endpoints reachable via the in-jail `HTTPS_PROXY` CONNECT shim → `SessionChannel` → host TCP relay (TLS end-to-end), same model as Claude; only the allowlisted egress host set differs.

### R2 — Managed codebase workflow for cursor-cli

- `StartSessionRequest.managed_codebase` + `recipe` + `specialized_agents` honored for `session_type = "cursor-cli"`.
- Seed `changeset.yaml`, launch with the orchestration system prompt. Cursor has no `--append-system-prompt-file`; the orchestration prompt is prepended to the initial user prompt (matching existing `CursorBackend` system-prompt handling) — or written to a file Cursor reads via `--` forwarding.
- Per-session `TDDY_SOCKET` so `tddy-tools transition` works from inside a cursor-cli session.
- Sandboxed managed path: repo **not** mounted in jail; reached only via `mcp__tddy-tools__*` calls relayed by host.

### R3 — Specialized subagents for cursor-cli

- `specialized_agents: [...]` resolved via `connection_service::resolve_specialized_agent_defs` (shared with Claude).
- `TDDY_SUBAGENTS_JSON` MCP subagent registry surfaced to the in-jail `tddy-tools --mcp`, exposing `subagent_new_session` / `subagent_prompt` / `subagent_cancel` tools.
- Works in both non-sandbox and sandbox (managed) cursor-cli sessions.

### R4 — MCP headless approval in `CursorBackend::invoke`

- `CursorBackend` registers `tddy-tools --mcp` + `permission-prompt-tool` for headless tool approvals, mirroring `ClaudeCodeBackend`.
- Wire `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, and `TDDY_REMOTE_*` env into the cursor invoke subprocess (today absent).

### R5 — Remote-codebase env in `CursorBackend::invoke`

- Export `RemoteToolEnv` (`TDDY_REMOTE_*`) from `CursorBackend::invoke_sync` so workflow-mode cursor invocations can run remote-codebase like Claude.

### R6 — `WaitingForInput` activity mapping

- If Cursor hooks expose a permission/await event (research Cursor's hook catalog during planning), map it to `SessionActivityStatus::WaitingForInput`. If no equivalent exists in Cursor's hook schema, document the gap and leave the no-op behavior (do **not** fabricate a mapping).

### R7 — Acceptance proof (seatbelt integration)

- After the GREEN phase, **manually** run the `agent` binary in the darwin sandbox via `tddy-sandbox-app` with a `"hi"` prompt and confirm the seatbelt integration works end-to-end (sandbox starts, `agent -p hi` runs confined, output returns, no sandbox denies on the happy path).
- Target command shape: `./dev cargo run -p tddy-sandbox-app -- --agent cursor --repo <repo> --cursor-binary agent -- -p hi` (exact flags finalized during planning).

## Success criteria

- [ ] `StartSession` with `session_type = "cursor-cli"` and `sandbox = true` **succeeds** (no `FAILED_PRECONDITION`) on macOS (Seatbelt) and Linux (cgroups+ns).
- [ ] Sandboxed cursor-cli session: `agent` runs confined; egress only via the `SessionChannel` tunnel; no `deny network*` violations on the happy path.
- [ ] `tddy-sandbox-app --agent cursor ... -- -p hi` runs the real `agent` binary in the darwin sandbox and returns output (manual proof executed and shown to user after GREEN).
- [ ] Managed-codebase cursor-cli session: `changeset.yaml` seeded, orchestration prompt applied, `TDDY_SOCKET` set, repo reachable only via `mcp__tddy-tools__*` in sandbox mode.
- [ ] Specialized subagents: `subagent_new_session`/`prompt`/`cancel` MCP tools available in cursor-cli sessions (sandbox and non-sandbox).
- [ ] `CursorBackend::invoke` registers MCP + `permission-prompt-tool`, exports `TDDY_SOCKET`/`TDDY_REPO_DIR`/`TDDY_SESSION_DIR`/`TDDY_REMOTE_*`.
- [ ] `WaitingForInput` mapped if Cursor exposes an equivalent hook event; otherwise the gap is documented (no fabricated mapping).
- [ ] All new acceptance tests pass on macOS and Linux; `./test` is green; `cargo clippy -- -D warnings` clean.

## Implementation plan overview

(Detailed technical plan + changeset produced in Plan mode via `/plan-ft-dev`.)

High-level milestones:

1. **Sandbox foundation (Cursor recipe)** — `tddy-sandbox-recipes::cursor_cli`, SBPL/cgroups profile, `sandbox-runner` agent spawn path.
2. **`tddy-sandbox-app` cursor mode** — `--agent cursor`, spawn path, terminal bridge reuse. **Acceptance proof executed here after GREEN.**
3. **Daemon sandboxed cursor-cli session** — `start_sandboxed_cursor_cli_session`, remove `FAILED_PRECONDITION`, `.session.yaml` sandbox fields.
4. **Managed codebase + specialized agents for cursor-cli** — recipe/orchestration prompt, `TDDY_SOCKET`, `TDDY_SUBAGENTS_JSON`, `resolve_specialized_agent_defs` reuse.
5. **`CursorBackend::invoke` parity** — MCP + permission-prompt-tool, env wiring, `RemoteToolEnv`.
6. **`WaitingForInput` mapping** — research Cursor hook catalog; map or document gap.
7. **Linux parity** — cgroups+namespace recipe for cursor-cli sandbox.
8. **Web/Telegram UX** — sandbox toggle, managed-codebase section, specialized-agents picker for cursor-cli in CreateSessionPane.
9. **Docs wrap** — update `cursor-cli-session.md`, managed-codebase/specialized-subagents docs, `tddy-sandbox-app` README.

## References

- [Cursor CLI session (v1)](../daemon/cursor-cli-session.md)
- [Claude Code CLI session](../daemon/claude-cli-session.md)
- [Managed codebase workflow](../coder/managed-codebase-workflow.md)
- [Managed codebase subagents](../coder/managed-codebase-subagents.md)
- [Specialized subagents](../coder/specialized-subagents.md)
- [Remote-codebase mode](../daemon/remote-codebase-mode.md)
- [Sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md)
- [darwin-sandbox skill](../../../.agents/skills/darwin-sandbox/SKILL.md)
- `packages/tddy-sandbox-app/src/main.rs` — standalone sandbox+Claude app (to be extended for `agent`)

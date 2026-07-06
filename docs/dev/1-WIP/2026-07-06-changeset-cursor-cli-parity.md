# Changeset: Cursor CLI Feature Parity with Claude CLI

**Date**: 2026-07-06
**Status**: Complete
**Type**: Feature

## Affected Packages

- `tddy-sandbox-recipes` — `cursor_cli` recipe
- `tddy-sandbox-runner` — `--agent-kind cursor` PTY spawn
- `tddy-sandbox-app` — `--agent-kind cursor` standalone proof
- `tddy-sandbox-cgroups` — Linux cursor-cli recipe parity
- `tddy-daemon` — `start_sandboxed_cursor_cli_session`, managed codebase + subagents
- `tddy-core` — `CursorBackend` MCP + `TDDY_REMOTE_*`
- `tddy-web` — CreateSessionPane sandbox/managed/agents for cursor-cli

## Related Feature Documentation

- PRD: [docs/ft/1-WIP/PRD-2026-07-06-cursor-cli-parity.md](../../ft/1-WIP/PRD-2026-07-06-cursor-cli-parity.md)
- [cursor-cli-session.md](../../ft/daemon/cursor-cli-session.md)
- [managed-codebase-workflow.md](../../ft/coder/managed-codebase-workflow.md)
- [specialized-subagents.md](../../ft/coder/specialized-subagents.md)

## Summary

Bring Cursor Agent CLI to parity with Claude CLI: Seatbelt/cgroups sandbox, managed codebase workflow, specialized subagents, MCP config via `.cursor/mcp.json` (headless approval flags only when caller passes them), and `CursorBackend::invoke` remote env wiring.

## Key Technical Decisions

1. **MCP parity**: Cursor has no `--mcp-config`; write `.cursor/mcp.json` under jail `$HOME`. Sandbox runner does not inject `--approve-mcps` / `--force` / `--trust` — callers pass them when needed (e.g. headless `-p`).
2. **WaitingForInput**: Documented gap — no Cursor hook equivalent.
3. **Proof**: `./dev cargo run -p tddy-sandbox-app -- --agent-kind cursor --repo <repo> --model composer-2.5 -- -p hi` after GREEN.

## Implementation Milestones

- [x] M1: `tddy-sandbox-recipes::cursor_cli`
- [x] M2: Runner `--agent-kind`
- [x] M3: `tddy-sandbox-app` cursor mode + manual proof
- [x] M4: Daemon sandboxed cursor-cli
- [x] M5: Managed codebase + specialized agents
- [x] M6: `CursorBackend::invoke` parity
- [x] M7: WaitingForInput documented gap
- [x] M8: Linux cgroups parity
- [x] M9: Web UX
- [x] M10: Docs wrap

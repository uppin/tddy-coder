# Headless Cursor, Codex and Oz run from their tool definitions - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 9/12` (branch `feature/agent-tools/headless-others`)

## Affected Features

- **Primary**: the `cursor` and `codex` workflow backends, which are declared in their specs. See [Coder overview](../1-OVERVIEW.md).
- **Related**: [Managed codebase subagents](../managed-codebase-subagents.md). The headless coding subagents for Cursor, Codex and Oz launch from specs.

## Summary

This node extends the `6/12` headless model to the remaining headless agents:

- **Cursor**: `cursor.yaml` gains a `headless:` section, using the `cursor-stream` adapter and `--list-models` parsing through `cursor-models`.
- **Codex**: `codex.yaml` is new. It runs `codex exec … --json` with the `codex-jsonl` adapter, declares MCP as an `inline` slot (`-c mcp_servers.<n>.*`). A sign-in failure keeps today's remediation hint; there is no login adapter, because this path has no login retry to move.
- **Oz**: a new `oz.yaml` (its own binary, `oz`) declares the headless `oz agent run`, with the `oz-ndjson` adapter and an `argv` MCP slot (`--mcp <path>`). `warp.yaml`, the interactive `warp` TUI, is unchanged.

The per-agent `headless_driver/argv.rs` builders are removed. `HeadlessHarness` stays, because the adapters' `turn_format` names it; `12/12` collapses it. The MCP file that a Cursor conversation writes is recorded with the conversation, so its close and the start-up sweep remove it ([2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md](../../../dev/todo/2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md)).

## Acceptance Criteria

- [ ] `tddy-coder --agent cursor` and `--agent codex` drive stubs whose argv equals today's, including resume and the thread id.
- [ ] The headless Cursor, Codex and Oz subagents launch with MCP rendered from each spec's slot kind: a worktree file (`workdir`), `inline` and `argv`.
- [ ] A closed Cursor conversation leaves no `.cursor/mcp.json`. A retry under the same conversation id is admitted.
- [ ] On a Codex auth error, today's `codex_openai_auth_remediation` hint is still appended. This is a preserved-behaviour pin.

## Out of Scope

- Native-tool withdrawal for these CLIs, which remains unproven ([2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md](../../../dev/todo/2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md)). The spec declares `tool_deny: unsupported`, which keeps today's refusal.
- ACP (`11/12`).

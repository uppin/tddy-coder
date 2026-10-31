# Headless Claude runs from its tool definition through a named stream adapter - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 6/12` (branch `feature/agent-tools/claude-headless`)

## Affected Features

- **Primary**: [Coder overview § backends](../1-OVERVIEW.md). The `claude` workflow backend's invocation is declared in `claude.yaml`.
- **Related**: [Managed codebase subagents](../managed-codebase-subagents.md). A headless Claude coding subagent (`HeadlessDriver`) launches from the same declaration.

## Summary

`claude.yaml` gains a **`headless:`** variant in its `coding_agent` extension. It declares:

- an argv template: `-p`, `--output-format stream-json --verbose`, the permission mode, the session id or resume, the system prompt and the slots;
- the **adapter** that reads its output: `adapter: claude-stream-json`.

A new **adapter registry** in `tddy-agent-backend` maps adapter names to the Rust stream parsers and turn drivers. Both consumers build Claude's invocation from the spec and run it through the named adapter:

- the workflow `CodingBackend` used by `tddy-coder --agent claude`;
- the coding-subagent `HeadlessDriver`.

The adapter is the **only** Claude-specific code left on these paths.

## Background

Headless Claude is hand-built in two places:

- `backend/claude.rs`, which is oversized, with `invoke_sync` over budget;
- `headless_driver/argv.rs::claude()`.

Its MCP temp file, permission-mode mapping and env are written in each. Moving the declaration into the spec removes both copies. It also proves the adapter-by-name model the developer chose for runtime protocol behaviour.

## Proposed Changes

### What's Changing

- The `coding_agent.headless` section: an argv template, env, `adapter`, and the mapping slots shared with the PTY variant.
- `tddy_agent_backend::adapters::{AdapterRegistry, StreamAdapter}`, with `claude-stream-json` registered. An unknown adapter name in a spec is refused when the spec loads.
- `ClaudeBackend::invoke` and `HeadlessDriver`'s Claude harness render argv from the spec.
  - Goal hints (plan mode, accept-edits) map onto the template's `permission_mode` parameter.
  - `claude_nonzero_exit_ok_if_structured_response` becomes an adapter property.

### What's Staying the Same

- Claude's headless argv, stream parsing, session-id and resume semantics, and the transcript-usage reader.
- Cursor, Codex and Oz headless (`9/12`).

## Acceptance Criteria

- [ ] `tddy-coder --agent claude` drives a stub `claude` whose recorded argv equals today's for plan and acceptEdits goals, including resume.
- [ ] A headless Claude coding subagent launches with the MCP config and approval tool rendered from the spec's slots.
- [ ] A `<tddyhome>/tools/claude.yaml` override changes the headless argv.
- [ ] A spec naming an unregistered adapter is refused, naming the adapter.

## Out of Scope

- Other headless agents (`9/12`).
- ACP (`11/12`).
- Collapsing `AnyBackend` (`12/12`).

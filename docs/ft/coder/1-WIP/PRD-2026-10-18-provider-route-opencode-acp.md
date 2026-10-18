# OpenCode as a tool-session agent over ACP - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 7/9 (base `feature/provider-route/muse-cli`)

## Affected Features

- **Primary Feature**: [ACP agent](../acp-agent.md) — a third ACP backend, `opencode`.
- **Related Feature**: [Tool session model selection](../../web/tool-session-model-selection.md) —
  `opencode` appears in the agent list and its models are listable.

## Summary

`opencode` becomes a tool-session agent: `tddy-coder --agent opencode` drives `opencode acp` over the
Agent Client Protocol, exactly as `claude-acp` and `codex-acp` drive theirs. On a provider account it
receives the generated OpenCode configuration of node 3 through the route file of node 4.

## Proposed Changes

### What's Changing
- `AnyBackend::OpenCode` (ACP over `opencode acp`), binary from `--opencode-cli-path` /
  `TDDY_OPENCODE_CLI` / coder config.
- Registration everywhere a backend is listed: selection question and label map, default and curated
  models (ACP `new_session().models`), context globs, `model_catalog::build_backend`,
  `BUILTIN_BACKEND_AGENT_IDS`, `tddy-tools list-models --opencode-cli-path`, `allowed_agents` in the dev
  and desktop configs.
- Route: tool-session route file applied as `OPENCODE_CONFIG` + key env on the ACP child.

### What's Staying the Same
Every other backend; the selection-question order of existing entries (OpenCode is appended).

## Acceptance Criteria
- [ ] `tddy-coder --agent opencode` completes an ACP turn against a fake `opencode acp`.
- [ ] `list-models --agent opencode` returns the agent's advertised models.
- [ ] On a Fireworks account the ACP child gets the generated config and key.
- [ ] Existing backend listings are unchanged apart from the appended entry.

## Out of Scope
OpenCode TUI (node 3); sandboxing tool sessions.

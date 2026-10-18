# Harness profiles: agent definitions that name a provider account - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 8/9 (base `feature/provider-route/opencode-acp`)

## Affected Features

- **Primary Feature**: [Session agent roster](../session-agent-roster.md) / agent definitions
  (`<tddyhome>/agents/*.yaml`) — a definition may describe a **harness profile**.
- **Related Feature**: [Tool session model selection](../../web/tool-session-model-selection.md) — profiles
  appear in the agent list.

## Summary

An agent definition can be a **harness profile**: `harness: claude | claude-acp | codex | codex-acp |
opencode | claude-cli | opencode-cli | muse-cli`, `provider_account: <account label or id>`, `model:`.
A profile shows up in the agent list (e.g. "Claude Code · Meta Wonder"); starting a session with it runs
that harness on that account and model. It is the agent-definition level of the route precedence:
an explicit session choice beats it, and it beats the person's default.

## Background

House preference: harness and provider choices are expressed through agent definitions rather than
dedicated flags. Today a definition is always tddy's own OpenAI-compatible loop (`base_url`, `api_key`).

## Proposed Changes

### What's Changing
- `SpecializedAgentDef` gains an optional `harness` profile block; a def with it is not an OpenAI loop
  and must not carry `base_url`/`api_key` (rejected on load).
- `ListAgents` rows for profiles; start resolves the profile to its session type / agent and feeds
  `RouteSources.agent_definition`. The account is resolved in the starting person's vault by label or id;
  missing → refused.

### What's Staying the Same
OpenAI-loop definitions and registry assistants; `replaces:` semantics.

## Acceptance Criteria
- [ ] A profile `harness: claude-cli, provider_account: meta, model: muse-wonder-1.0` starts a claude-cli
      session on that account and model.
- [ ] A session-level account choice overrides the profile's.
- [ ] A profile naming an account the person does not hold is refused with the reason.
- [ ] A profile with `base_url` is rejected at load.

## Out of Scope
Profile editing in the UI (YAML only in this node).

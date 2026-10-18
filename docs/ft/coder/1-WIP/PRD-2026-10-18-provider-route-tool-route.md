# Tool sessions with Claude and Codex agents on a provider account - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 4/9 (base `feature/provider-route/opencode-cli`)

## Affected Features

- **Primary Feature**: [ACP agent](../acp-agent.md), [Codex ACP backend](../codex-acp-backend.md)
  — tool-session backends may run on a provider account.
- **Related Feature**: [Tool session model selection](../../web/tool-session-model-selection.md) — the
  provider picker applies to tool sessions.

## Summary

A **tool session** (headless `tddy-coder`) whose agent is `claude`, `claude-acp`, `codex` or `codex-acp`
may be started on a provider account. Claude agents get the Anthropic route of node 2. Codex agents get
a Responses-API provider (`model_providers.<id>` with `wire_api = "responses"`) pointing at the account,
so Codex runs Muse models on Meta. Cursor stays unsupported.

## Background

A tool session's process is spawned by the daemon with a closed environment allowlist (git identity
only — "never a credential") and, under the supervisor, a refuse-on-unlisted policy. The established way
a credential reaches `tddy-coder` is a 0600 file (`--agent-def-path`). This node delivers the route the
same way and has `tddy-coder` apply it to the backend's child process.

## Proposed Changes

### What's Changing
- The daemon writes the resolved route to `<session_dir>/provider-route.json` (0600, chowned) and
  passes `tddy-coder --provider-route-path <file>`; resume does the same from `.session.yaml`.
- `tddy-coder` loads it and hands it to the backend: headless `claude` and `claude-acp` set the route's
  env on their child; `codex` adds `-c model_provider=… -c model_providers.<id>.{name,base_url,env_key,
  wire_api}` and sets the key under `env_key`; `codex-acp` gets the same overrides through its own
  config mechanism.
- An agent the route does not support (cursor, stub, a specialized def) refuses the start with the
  reason.
- Web: the provider picker is shown for tool sessions and filtered by `ListProviderRoutes(agent)`.

### What's Staying the Same
Tool sessions without an account; the git-identity env allowlist; supervisor policy (no new env keys).

## Acceptance Criteria
- [ ] A `codex` tool session on a Meta account runs `codex exec` with the Meta Responses provider and
      the chosen model; the key is in the child env only.
- [ ] A `claude` tool session on Fireworks runs `claude -p` with the Fireworks Anthropic route.
- [ ] `claude-acp` and `codex-acp` spawn their agents with the route applied.
- [ ] The route file is mode 0600 and owned by the session user; no key appears in argv.
- [ ] A `cursor` tool session with an account is refused with the reason.
- [ ] Resume reapplies the route.

## Out of Scope
OpenCode as a tool-session agent (node 7); defaults (node 5); agent definitions (node 8).

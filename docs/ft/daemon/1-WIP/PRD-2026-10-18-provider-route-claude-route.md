# Claude CLI sessions on a connected provider account - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 2/9 (base `feature/provider-route/accounts`)

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) — may run on a provider account
  instead of Anthropic.
- **Related Feature**: [Tool session model selection](../../web/tool-session-model-selection.md) — the
  Create-session pane gains a provider-account picker; the model list follows the chosen account.
- **Related Feature**: [Cursor CLI session](../cursor-cli-session.md) — shown as not routable, with the reason.

## Summary

Starting a **claude-cli** session, the operator may pick one of their provider accounts. The session's
Claude Code then talks to that account's Anthropic-Messages endpoint with that account's key, and every
model Claude Code may choose — main loop, the opus/sonnet/haiku aliases and subagents — is pinned to a
model the account serves, so no request reaches Anthropic. The choice is stored with the session and a
resume uses the same account. This node runs unsandboxed claude-cli sessions; jailed ones are node 9.

## Background

Node 1 stores provider accounts. Meta's Model API and Fireworks both serve Anthropic Messages, which is
what Claude Code speaks when `ANTHROPIC_BASE_URL` / `ANTHROPIC_AUTH_TOKEN` are set. Today no harness is
ever pointed anywhere but its vendor. This node introduces the **route** — the per-harness translation
of (account, model) into what the harness process needs — that the rest of the stack reuses.

## Proposed Changes

### What's Changing
- `StartSessionRequest.provider_account_id` (optional). Absent = today's behaviour.
- **Route resolver** (pure): `(harness, account endpoints, key, model) → HarnessRoute { env, secret env,
  files, args }`, with a **capability matrix**: Claude Code needs Anthropic Messages; Codex needs OpenAI
  Responses; OpenCode accepts Responses or Chat; Muse needs the `meta` preset; Cursor accepts nothing.
  An unsupported pair is refused with the reason — never silently ignored.
- **Route precedence** `RouteSources { session, agent_definition, user_default }` → the one choice
  (session > agent definition > user default). Only the session level is fed in this node.
- **`ListProviderRoutes(harness)`** RPC: the caller's accounts with, per account, whether it can drive
  that harness and why not. The web never re-implements the matrix.
- claude-cli start: the account's key is read from the owner's vault at start (refusing if locked or
  missing), and the PTY gets `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_MODEL`,
  `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`.
- `.session.yaml` persists `provider_account_id`; claude-cli resume reapplies the route.
- Web: Create-session pane shows "Provider" (default "Anthropic (built in)") for claude-cli; picking an
  account switches the model list to `ListProviderAccountModels`. Cursor CLI shows the picker disabled
  with "Cursor's agent cannot use another provider".

### What's Staying the Same
- Sessions without an account behave exactly as today (host `.credentials.json`, OAuth token).
- Cursor, Codex, tool sessions, sandboxed claude-cli: unchanged in this node.

## Impact Analysis

### Technical Impact
`session.proto` (+1 field), `catalog.proto` (+1 RPC), `tddy-agent-backend` (route resolver module),
`tddy-agent-launch` (start/resume for claude-cli; vault read through `SessionAccountAccess`),
`tddy-changeset` (`.session.yaml` field), `tddy-daemon-rpc` (catalog handler), `tddy-web`
(Create-session pane). The key reaches only the PTY child's environment.

### User Impact
New option on Create session; default unchanged. A session on an account fails to start while the
vault is locked (no fallback to Anthropic).

## Acceptance Criteria
- [ ] A claude-cli session started on a Meta account runs Claude Code with the Meta endpoint, the
      account's key and every model alias pinned to the chosen model.
- [ ] A locked vault refuses the start with the vault-locked reason; no process is spawned.
- [ ] A Baseten account (no Anthropic Messages) is refused for claude-cli with the reason.
- [ ] Resume of that session uses the same account and model.
- [ ] `ListProviderRoutes("cursor-cli")` marks every account unsupported with the reason.
- [ ] The pane lists the account's models once an account is chosen; Create sends the account id.
- [ ] A session without an account starts exactly as before.

## Out of Scope
Tool sessions (node 4), sandboxed sessions (node 9), user defaults (node 5), agent definitions (node 8).

## References
Meta: https://dev.meta.ai/docs/coding-agents (Claude Code section); Fireworks Anthropic compatibility.

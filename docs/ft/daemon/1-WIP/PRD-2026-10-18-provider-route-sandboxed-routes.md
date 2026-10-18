# Sandboxed CLI sessions on a provider account - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 9/9 (base `feature/provider-route/agent-def-route`)

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) (sandboxed mode) — honours a provider
  route.
- **Related Feature**: new OpenCode and Muse Code session docs (nodes 3, 6) — gain sandboxed mode.

## Summary

Jailed claude-cli sessions run on a provider account, and OpenCode and Muse Code sessions can be
sandboxed. Inside the jail the account key is delivered through the existing secret-file channel
(`TDDY_SECRET_<NAME>` → set on the agent child only, file unlinked); non-secret route values are part of
the jail's explicit environment; route files are materialised into the jail's scratch space.

## Proposed Changes

### What's Changing
- Recipe secret injection generalised from the single `CLAUDE_CODE_OAUTH_TOKEN` case to a route's
  secret env (`ANTHROPIC_AUTH_TOKEN`, Codex/OpenCode key variables, Muse key).
- Sandboxed claude-cli start applies the route; no OAuth token is injected when a route is present.
- Runner `--agent-kind opencode | muse` with their PTY spawns; recipes, read-allow paths, and the
  sandbox app's `AgentKind` learn both.

### What's Staying the Same
Unrouted sandboxed claude-cli and cursor-cli; jail egress (no host allowlist exists; provider hosts are
reachable through the shim as Anthropic is today).

## Acceptance Criteria
- [ ] A sandboxed claude-cli session on Meta: the in-jail Claude Code has the route env and the key;
      the key never appears in the jail env map or on argv.
- [ ] A sandboxed OpenCode session on Fireworks starts with its generated config inside the jail.
- [ ] A sandboxed Muse Code session on Meta starts with the key.
- [ ] Unrouted sandboxed sessions are unchanged.

## Out of Scope
An egress host allowlist; sandboxed tool sessions.

# OpenCode terminal sessions - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 3/9 (base `feature/provider-route/claude-route`)

## Affected Features

- **Primary Feature**: new `docs/ft/daemon/opencode-cli-session.md` — OpenCode's TUI in a terminal pane.
- **Related Feature**: [Terminal sessions](../terminal-sessions.md) — a third CLI session type beside
  claude-cli and cursor-cli.
- **Related Feature**: [Claude CLI session](../claude-cli-session.md) — the session-type plumbing it shares
  moves behind one harness descriptor.

## Summary

The Create-session pane offers **OpenCode** beside Claude CLI and Cursor CLI. An `opencode-cli` session
runs OpenCode's interactive TUI in a PTY (unsandboxed in this node) in the session worktree, and is
resumable. On a provider account, OpenCode is given a generated configuration naming that account's
endpoint (Responses preferred, Chat Completions otherwise), key and model; without one it uses the
person's own OpenCode setup.

## Background

OpenCode is the provider-agnostic harness: it is the natural way to run Fireworks, Baseten, Modal and
generic endpoints that serve neither Anthropic Messages nor Responses. Adding a third CLI session type
by hand would mean another arm in ~30 string matches; this node puts a **CLI harness descriptor**
behind those matches so OpenCode, and Muse Code after it, register instead of being copied in.

## Proposed Changes

### What's Changing
- `session_type = "opencode-cli"`: start, resume, model validation, `.session.yaml`, session listing,
  terminal relay, context files (`AGENTS.md`, `opencode.json`, `.opencode/**`).
- **CLI harness descriptor**: per session type — binary (config + env override), argv for (model,
  resume id, initial prompt), route harness id, context agent, model-catalog source. claude-cli and
  cursor-cli are expressed through it without behaviour change.
- OpenCode route: a per-session `opencode.json` (0600, in the session dir) declaring the account as a
  provider (`@ai-sdk/openai` for Responses with reasoning carried across turns, `@ai-sdk/openai-compatible`
  otherwise) and selecting it; delivered by `OPENCODE_CONFIG`; the key only in the child's env.
- Model list: from the account when one is chosen, else `opencode models`.
- Daemon config `opencode_cli.binary_path`; web toggle button "OpenCode".

### What's Staying the Same
claude-cli and cursor-cli behaviour, argv and env (pinned by their existing tests).

## Acceptance Criteria
- [ ] Creating an OpenCode session spawns `opencode` in the worktree with the chosen model.
- [ ] On a Fireworks account, the session's config names the Fireworks endpoint and the model, and the
      key reaches only the child.
- [ ] Resume reopens the same OpenCode session id.
- [ ] Without an account, no config is generated.
- [ ] claude-cli and cursor-cli starts are byte-identical in argv/env to before (existing suites green).
- [ ] The pane shows OpenCode and its model list.

## Out of Scope
Sandboxed OpenCode (node 9); OpenCode as a headless tool-session agent (node 7).

## References
https://opencode.ai/docs ; Meta coding-agents guide (OpenCode section).

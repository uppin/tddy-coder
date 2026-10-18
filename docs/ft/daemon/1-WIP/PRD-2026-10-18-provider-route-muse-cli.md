# Muse Code terminal sessions - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 6/9 (base `feature/provider-route/default-route`)

## Affected Features

- **Primary Feature**: new `docs/ft/daemon/muse-cli-session.md` — Meta's Muse Code TUI in a terminal pane.
- **Related Feature**: [Terminal sessions](../terminal-sessions.md) — a fourth CLI session type, registered
  through the harness descriptor of node 3.

## Summary

The Create-session pane offers **Muse Code**. A `muse-cli` session runs Meta's `muse` TUI in a PTY
(unsandboxed in this node) in the session worktree. On a Meta provider account, the account's key is
handed to Muse Code; without one, Muse Code uses the person's own sign-in. Models: the Muse family the
account serves (e.g. `muse-spark-1.3`, `muse-wonder-1.0`).

## Background

Meta's own harness is tuned for its models and is the hackathon's showcase tool. Its headless protocol
is undocumented, so only the interactive TUI is integrated — argv and env only, no stream parsing.

## Proposed Changes

### What's Changing
- `session_type = "muse-cli"` registered as a CLI harness descriptor: binary `muse` (config
  `muse_cli.binary_path`), `--model`, resume, initial prompt, context files (`AGENTS.md`,
  `.agents/skills/**`, `.muse/**`).
- Route for harness `muse`: only the `meta` preset; key delivered as the variable Muse Code reads for
  non-interactive auth (confirmed against `muse --help` in wave 2 — secondary sources say `META_API_KEY`).
- Model list from the Meta account; without an account, the curated Muse list.
- Web toggle button "Muse Code".

### What's Staying the Same
All other session types; the descriptor itself (owned by node 3).

## Acceptance Criteria
- [ ] Creating a Muse Code session on a Meta account spawns `muse` with the model and the key in env.
- [ ] A Fireworks account is refused for Muse Code with the reason.
- [ ] Resume continues the same Muse session.
- [ ] Without an account the session starts with no key injected.

## Out of Scope
A headless `muse exec --json` backend (backlog); sandboxed Muse (node 9).

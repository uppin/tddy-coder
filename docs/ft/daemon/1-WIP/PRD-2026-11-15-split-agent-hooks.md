# A split session's agent reports its status, questions and notifications - PRD

**Date**: 2026-11-15
**PRD Type**: Bug Fix
**Stack**: `#split-alive` 2/4 — branch `feature/split-alive/split-hooks`

## Affected Features

- [Remote managed worktree](../remote-managed-worktree.md) — § Session start (the agent's launch)
- [Agent session status](../agent-session-status.md) — fed by the agent's lifecycle hooks
- [Session notifications](../session-notifications.md) — "waiting for input", turn ended, questions

## Summary

A split session's agent never reports anything: its status stays `active`, its questions and
permission prompts never reach the orchestrator or the web, and no notification is ever raised. The
daemon writes the agent's hooks into `.claude/settings.local.json` in its context directory, but
launches it with `--setting-sources user` — which Claude Code honours by not loading that file. After
this change the split agent loads its hooks explicitly, and reports like every other agent. Its
environment also gains the agent spec's own launch variables, as co-located agents already get.

## Background

Found during the 2026-11-15 investigation
([findings](../../../dev/todo/2026-11-15-split-session-agent-not-running.md), fix 3), verified with
Claude Code 2.1.311: a `settings.local.json` hook does not fire under `--setting-sources user`, fires
under `user,project,local`, and fires under `--setting-sources user --settings <that file>`.

`--setting-sources user` is there on purpose: it keeps a checkout's own `.claude/settings.json` —
whose hooks would run on the operator's host — out of an agent whose codebase lives elsewhere. Adding
`local` would load whatever `settings.local.json` the context sync brought over. Passing the
daemon-written file by name keeps that isolation and loads exactly what the daemon wrote.

## Proposed Changes

### What's Changing

1. **The split agent loads its hooks file.** Its launch carries `--settings <context>/.claude/settings.local.json`.
   Which flag loads a hooks file is declared by the agent's spec, on its `host_agent` variant, not
   hard-coded for Claude.
2. **The split agent's env carries its spec's `run.env`** (for Claude, `CLAUDE_CODE_NO_FLICKER=1`, the
   renderer the web terminal scrolls with), like a co-located agent's.

### What's Staying the Same

- `--strict-mcp-config` and `--setting-sources user`: the checkout still configures nothing here.
- The hooks file's content, location and the hook token.
- Co-located and jailed launches.

## Impact Analysis

### Technical Impact

- `tddy-coding-agent-specs`: the `host_agent` variant declares the flag that loads a hooks file;
  `claude.yaml` declares `--settings`.
- `tddy-session-split`: the split launch adds that flag with the hooks file's path, and passes the
  spec's `run.env` into the agent env.

### User Impact

- A split session shows `WaitingForInput`, questions, permission prompts and turn ends in the web, in
  `ListProjectSessions`, `ReadSessionActivity` and project notifications.

## Success Criteria

- [ ] A split agent's launch argv carries `--settings` naming the hooks file the daemon wrote into its context dir.
- [ ] The split agent's env carries `CLAUDE_CODE_NO_FLICKER=1`.
- [ ] A variant that declares no hooks flag renders no such argument (other agents unchanged).
- [ ] `--setting-sources user` and `--strict-mcp-config` are still on the argv.

## Out of Scope

- The agent's startup and exit reporting (`#split-alive` 1/4).
- Inspection tools (3/4); start result and drawer (4/4).
- Muse Code's hooks ([2026-11-14-muse-hooks-not-wired.md](../../../dev/todo/2026-11-14-muse-hooks-not-wired.md)).

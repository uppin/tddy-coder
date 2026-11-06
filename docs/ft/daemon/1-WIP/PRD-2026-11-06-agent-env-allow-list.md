# Agents start from an allow-listed environment — PRD

**Date**: 2026-11-06
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 28/28 — branch `feature/agent-tools/env-allow-list`

## Affected Features

- [claude-cli-session.md](../claude-cli-session.md), [cursor-cli-session.md](../cursor-cli-session.md),
  [opencode-cli-session.md](../opencode-cli-session.md), [muse-cli-session.md](../muse-cli-session.md),
  [warp-cli-session.md](../warp-cli-session.md) — the agent process environment
- [harness-profiles.md](../harness-profiles.md) — agent specs (`tools/*.yaml`)

## Summary

An agent process starts from an **empty** environment and gets only (1) a small fixed base set,
(2) the variables its spec declares it needs, and (3) what the launch sets on purpose (route keys,
spec `env`). Today every host launch path passes the daemon's whole environment minus `TDDY_*`
(or does not filter at all), so anything the daemon was started with reaches the agent. That
includes the `CLAUDE_CODE_*` markers that turned off transcript saving in a desktop-dev run.

## Background

Discovery for this stack found these host construction sites inheriting the daemon's env:

- deny-list: `turn_process`, `spec_headless` model listing, `spec_launch` (`env -u`, tmux unset),
  `jail_launch`;
- inherit-all: the ACP worker, `conversation_id` mint, `tddy-spawn`'s spawner.

The sandbox runner and sandbox-cgroups already use `env_clear` with an allow-list.

## Proposed Changes

### What's Changing

- One shared constructor builds every agent child env: an `env_clear`, then:
  - **base:** `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `LC_*`, `TMPDIR`, `TZ`,
    `SSH_AUTH_SOCK`, `HTTP(S)_PROXY`/`NO_PROXY` (both cases), `XDG_*`;
  - **spec-declared:** a new `pass_env:` list on the tool spec (names or `PREFIX_*`), for provider
    keys and agent-specific variables such as `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and
    `CURSOR_API_KEY`;
  - **set on purpose:** spec `env`, route keys and the launch's own variables, as today.
- Every host path above uses it: headless turns, model listing, PTY/tmux launch, ACP worker, the
  conversation-id mint and the spawner's agent spawns.
- `TDDY_*` can never pass. A `pass_env` entry matching `TDDY_*` is a spec load error.

### What's Staying the Same

- Jailed launches: the runner already builds an allow-listed env.
- What spec `env` and route keys set.
- Tool (action) processes, which are not agents.

## Impact Analysis

### Technical Impact

`tddy-coding-agent-specs` (`pass_env` schema and each built-in spec), `tddy-agent-backend`,
`tddy-agent-launch`, `tddy-spawn`. A variable an agent needed but nobody declared stops reaching
it, so each built-in spec's `pass_env` must be filled in from what the agent reads.

### User Impact

An agent no longer sees the daemon's environment: credentials, `CLAUDE_CODE_*` session markers, CI
tokens. An operator whose agent needs an extra variable adds it to `pass_env` in
`tools/<agent>.yaml`.

## Implementation Plan

1. `pass_env` schema and validation; fill in the built-in specs.
2. Shared allow-list constructor plus unit tests.
3. Convert each host construction site, with a test per site that a planted daemon variable does not reach the child.

## Acceptance Criteria

- [ ] A variable set in the daemon's env and not allow-listed (e.g. `CLAUDE_CODE_CHILD_SESSION`) reaches no agent child on any host launch path: headless turn, PTY, tmux, ACP, model listing.
- [ ] A variable named in the spec's `pass_env` does reach it.
- [ ] Base variables reach it.
- [ ] A `pass_env` entry matching `TDDY_*` fails spec loading, naming the spec.
- [ ] Every built-in spec declares the provider-key variables its agent reads.

## References

### Affected Features (Complete List)

- `docs/ft/daemon/claude-cli-session.md`, `cursor-cli-session.md`, `opencode-cli-session.md`,
  `muse-cli-session.md`, `warp-cli-session.md`, `harness-profiles.md`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-06-agent-env-allow-list.md`

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
- inherit-all: the ACP worker, the `conversation_id` mint, the PTY runtime (every agent main, the split agent).
- the supervisor's relayed spawn: every key gated on `allowed_env_keys`, plus its own `PATH`/locale.

The sandbox runner and sandbox-cgroups already use `env_clear` with an allow-list.

## Proposed Changes

### What's Changing

- The daemon computes every agent child's whole env in one place, and the child starts from exactly that, with an `env_clear`:
  - **base (YAML):** the built-in `agent-env.yaml`, which an operator overrides whole with `<tddyhome>/tools/agent-env/base.yaml`. Default: `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `LC_*`, `TMPDIR`, `TZ`,
    `SSH_AUTH_SOCK`, `HTTP(S)_PROXY`/`NO_PROXY` (both cases), `XDG_*`;
  - **spec-declared:** a new `pass_env:` list on the tool spec (names or `PREFIX_*`), for provider
    keys and agent-specific variables such as `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and
    `CURSOR_API_KEY`;
  - **set on purpose:** spec `env`, route keys and the launch's own variables, as today.
- Every host path above uses it: headless turns, model listing, PTY/tmux launch, ACP worker, the
  conversation-id mint, the split agent's PTY, and the supervisor's relayed spawn.
- An inherited `TDDY_*` can never pass. A `pass_env` or base-set entry matching `TDDY_*` is a load error. One `TDDY_*` the launch sets on purpose, Codex's `TDDY_CODEX_OAUTH_OUT`, is a documented exception until the Codex OAuth stack.
- The supervisor applies no coding-agent env rules to a relayed agent. It passes the daemon-computed env through, keeping only its own concerns: the OS user's `HOME`/`USER`/`LOGNAME`, cgroups, and refusing loader variables.

### What's Staying the Same

- Jailed launches: the runner already builds an allow-listed env.
- What spec `env` and route keys set.
- Tool (action) processes, which are not agents.

## Impact Analysis

### Technical Impact

`tddy-coding-agent-specs` (`pass_env` schema and each built-in spec), `tddy-agent-backend`,
`tddy-agent-launch`, `tddy-pty`, `tddy-cli-sessions`, `tddy-session-split`, `tddy-supervisor`. A variable an agent needed but nobody declared stops reaching
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

- [ ] A variable set in the daemon's env and not allow-listed (e.g. `CLAUDE_CODE_CHILD_SESSION`) reaches no agent child on any host launch path: headless turn, PTY, tmux, ACP, model listing, conversation-id mint, split agent, supervisor relay.
- [ ] A variable named in the spec's `pass_env` does reach it.
- [ ] Base variables reach it.
- [ ] A `pass_env` entry matching `TDDY_*` fails spec loading, naming the spec.
- [ ] An operator override of the base set is honoured; a `TDDY_*` entry in it fails loading, naming the file.
- [ ] A relayed agent receives exactly the daemon-computed env; no supervisor key list is involved.
- [ ] A user's Bash terminal still inherits the daemon's environment.
- [ ] Every built-in spec declares the provider-key variables its agent reads.

## References

### Affected Features (Complete List)

- `docs/ft/daemon/claude-cli-session.md`, `cursor-cli-session.md`, `opencode-cli-session.md`,
  `muse-cli-session.md`, `warp-cli-session.md`, `harness-profiles.md`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-06-agent-env-allow-list.md`

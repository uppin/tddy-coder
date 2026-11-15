# Changeset: A split session's agent reports its status, questions and notifications

**Date**: 2026-11-15
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — two crates (`tddy-coding-agent-specs` YAML schema + `tddy-session-split`), start and resume paths, unit and wiring-level tests only
**Stack**: `#split-alive` 2/4 — branch `feature/split-alive/split-hooks`, base `feature/split-alive/agent-liveness`
**PR**: _(set in pass 2)_

## Initial Discovery

[2026-11-15-split-agent-hooks-initial-discovery.md](./2026-11-15-split-agent-hooks-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-15-split-session-agent-not-running.md](../todo/2026-11-15-split-session-agent-not-running.md) | Referenced — claimed by `feature/split-alive/agent-liveness`; this node closes its **fix 3** (hooks) and the `run.env` divergence noted in § A.3 |
| [packages/tddy-coding-agent-specs/docs/code-issues/complexity-slots-mod-render.md](../../../packages/tddy-coding-agent-specs/docs/code-issues/complexity-slots-mod-render.md) (65 lines) | ⚠ **During** — `slots::render` is not touched; the flag is rendered by a new `CodingAgentSpec::variant_hooks_args` beside `variant_args` |
| [2026-11-14-muse-hooks-not-wired.md](../todo/2026-11-14-muse-hooks-not-wired.md) | — Unrelated (Muse agent, co-located) |

## Affected Packages

- **tddy-coding-agent-specs**: [README.md](../../../packages/tddy-coding-agent-specs/README.md) — `coding_agent.variants.<name>.hooks_flag`; `claude.yaml` declares `--settings` on `host_agent`
- **tddy-session-split**: [README.md](../../../packages/tddy-session-split/README.md) — the split launch (start and resume) loads its hooks file and carries the spec's `run.env`

## Related Feature Documentation

- [PRD-2026-11-15-split-agent-hooks.md](../../ft/daemon/1-WIP/PRD-2026-11-15-split-agent-hooks.md)
- [remote-managed-worktree.md § Session start](../../ft/daemon/remote-managed-worktree.md#session-start)

## Summary

The `host_agent` variant declares the flag that loads a hooks file (`hooks_flag: --settings` for
Claude). Every split launch — start and resume, both built by `split_claude_extra_args` — passes it
with the hooks file the daemon writes into the context dir, and the split agent's process env carries
the spec's `run.env`.

## Background

PRD § Background. Verified with Claude Code 2.1.311 (2026-11-15): `settings.local.json` hooks do not
fire under `--setting-sources user`; they do with `--settings <file>` added.

## Responsibility

- `AgentVariant.hooks_flag: Option<String>` (`tddy-coding-agent-specs/src/spec.rs`), validated like
  any flag; `claude.yaml` `variants.host_agent.hooks_flag: --settings`.
- `CodingAgentSpec::variant_hooks_args(&self, name, hooks_file: &Path) -> Result<Vec<String>, VariantError>`
  — `[flag, path]` when the variant declares one, empty otherwise.
- `tddy_session_split::agent_argv::split_hooks_file(session_dir) -> PathBuf` — the path the hooks slot
  renders the hooks file to in the split context dir (the same path `write_split_hooks` writes).
- `split_claude_extra_args` appends `variant_hooks_args(HOST_AGENT_VARIANT, split_hooks_file(..))`.
- The split agent's env: `spec.run.env` added on the start path (`split_agent_pty.rs`) and returned in
  `SplitAgentWiring.env` on the resume path (`prepare_split_agent_wiring`).

## Boundaries

- Does **not** change `--setting-sources user` or `--strict-mcp-config`.
- Does **not** change the hooks file's content, the hook token or `write_split_hooks`.
- Does **not** touch `slots::render`, co-located or jailed launches.
- Does **not** touch agent liveness, recording or exit logging (`feature/split-alive/agent-liveness`).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/split-alive/agent-liveness` (#905) | startup watch and recording in `svc_spawn_split_agent/startup.rs`; exit status in `tddy-pty` | nothing — only the branch line | edit `startup.rs`, `spawn_split_agent`'s startup calls, or `tddy-pty` |

## Draft PR contract

First push (pass 2, commit 2): `AgentVariant.hooks_flag`, `variant_hooks_args` and `split_hooks_file`
as stubs (`// TODO(split-hooks): implement`), `claude.yaml` unchanged until green, plus the failing
tests below.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes — tests drive the spec and the split wiring directly, no daemon
**Concurrent with:** #905, `feature/split-alive/split-targets`, `feature/split-alive/split-pairing`
**Blocks:** none

Real dependency edges: none.

## Scope

- [ ] `host_agent` variant declares its hooks flag; Claude's is `--settings`
- [ ] Split start and resume pass the hooks file with that flag
- [ ] Split agent env carries the spec's `run.env` on start and resume
- [ ] Docs: `remote-managed-worktree.md` § Session start names the flag and why `--setting-sources user` stays

## Technical Changes

### State A

- `claude.yaml:166-168` `host_agent: { args: [--strict-mcp-config, --setting-sources, user], withdraw_native: [...] }`.
- `spec.rs:372-379` `AgentVariant { args, withdraw_native }` (`deny_unknown_fields`); `variant_args` `:397-424`.
- `agent_argv.rs:126-186` `split_claude_extra_args`, called by the start (`svc_spawn_split_agent.rs:360`)
  and by `prepare_split_agent_wiring` (`split_session.rs:369`, resume via `svc_resume_split_wiring.rs:91`).
- Hooks rendered to `<context>/.claude/settings.local.json` by `launch.rs:133-172`.
- Start env `split_agent_pty.rs:30` `agent_env(spec, vars_os(), &[])`; resume `SplitAgentWiring.env = Vec::new()`
  (`split_session.rs:400`).

### State B

As in § Responsibility. The resume path's spawn (`tddy-agent-launch/src/opencode_cli_spawn/resume.rs:231`)
already extends its env with `wiring.env`, so it needs no change.

### Delta

| Package | Change |
|---|---|
| `tddy-coding-agent-specs` | `AgentVariant.hooks_flag`, `variant_hooks_args`, `claude.yaml` |
| `tddy-session-split` | `split_hooks_file`; `split_claude_extra_args`; `run.env` on start and resume |
| docs | `remote-managed-worktree.md` |

## Implementation Milestones

- [ ] Spec field + renderer + YAML, unit tests green
- [ ] Split args carry the flag (start and resume), wiring tests green
- [ ] Split env carries `run.env`, tests green

## Testing Plan

Unit tests in `tddy-coding-agent-specs` for the field and its rendering; wiring-level acceptance in
`tddy-session-split/tests/split_agent_wiring_acceptance.rs` (`prepare_split_agent_wiring`, no daemon, no
LiveKit) for start and resume flags; a unit test beside `split_agent_pty.rs` for the start env. That
the hook *fires* is Claude's behaviour, verified once by hand (2026-11-15) and recorded in the PRD.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_split_agent_loads_the_hooks_file_its_daemon_writes_as_settings` | `packages/tddy-session-split/tests/split_agent_wiring_acceptance.rs` | `extra_args` holds `--settings <session_dir>/context/.claude/settings.local.json` |
| `a_split_agent_still_keeps_the_checkouts_settings_out` | same | `--setting-sources user` and `--strict-mcp-config` remain |
| `a_resumed_split_agent_carries_its_specs_launch_variables` | same | `wiring.env` holds `CLAUDE_CODE_NO_FLICKER=1` and still no `TDDY_*` |

## Unit / Integration Tests

| Test | File |
|---|---|
| `a_variant_declaring_a_hooks_flag_renders_it_with_the_file`, `a_variant_without_a_hooks_flag_renders_nothing`, `claudes_host_agent_loads_hooks_with_settings` | `packages/tddy-coding-agent-specs/src/spec.rs` (tests) |
| `the_split_agent_pty_env_carries_the_specs_run_env` | `packages/tddy-session-split/src/svc_spawn_split_agent/split_agent_pty_tests.rs` |

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap docs

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **`--settings <file>` over `--setting-sources user,local`** (user decision, 2026-11-15): keeps the
  checkout's `settings.local.json`, if synced into the context dir, from configuring the agent.
- **Declared in the spec**, not hard-coded: another agent's variant can name its own flag or none.

## Refactoring Needed

### From /validate-changes

### From /pr-wrap

## Validation Results

### /validate-changes

### /pr-wrap

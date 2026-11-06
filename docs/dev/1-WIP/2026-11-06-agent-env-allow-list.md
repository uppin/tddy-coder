# Changeset: Agents start from an allow-listed environment

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — spec schema contract (`pass_env`) + four crates + every host launch path; a missed variable breaks an agent at runtime

## Initial Discovery

[2026-11-06-agent-env-allow-list-initial-discovery.md](./2026-11-06-agent-env-allow-list-initial-discovery.md)

## Prerequisites

No todo record — the rule ("agents start from a clean env, not the daemon's env minus `TDDY_*`")
was collected as a sweep item on 2026-11-06 after a desktop-dev run passed `CLAUDE_CODE_*` markers
to agents. No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-coding-agent-specs** — `pass_env:` on the tool spec; `TDDY_*` entries a load error; built-in specs declare their agents' variables.
- **tddy-agent-backend** — `turn_process.rs`, `spec_headless.rs` (model listing), `acp_driver/worker.rs`, `adapters/conversation_id.rs` use the shared constructor.
- **tddy-agent-launch** — `spec_launch.rs` (`env -u` and tmux unset paths), `jail_launch.rs:346`.
- **tddy-spawn** — `spawner.rs:1054-1061` agent spawns.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-agent-env-allow-list.md](../../ft/daemon/1-WIP/PRD-2026-11-06-agent-env-allow-list.md)

## Summary

Every host agent child env is `env_clear` + base set + spec `pass_env` + what the launch sets on
purpose. `TDDY_*` can never pass.

## Scope

- [ ] `pass_env` schema (names and `PREFIX_*`), `TDDY_*` refused at load
- [ ] Built-in specs declare provider keys and agent variables they read
- [ ] One shared constructor; base set as approved (2026-11-06): `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `LC_*`, `TMPDIR`, `TZ`, `SSH_AUTH_SOCK`, `HTTP(S)_PROXY`/`NO_PROXY` (both cases), `XDG_*`
- [ ] Converted: `turn_process`, model listing, ACP worker, conversation-id mint, `spec_launch` (direct and tmux), `jail_launch` host side, `tddy-spawn` agent spawns
- [ ] Deny-list helpers (`strip_tddy_env`, `without_inherited_tddy_variables`, `in_a_pane_without_inherited_tddy`) deleted

## Technical Changes

### State A

Exploration 2 table "Agent-child env construction sites". tmux panes inherit the tmux server's env,
so the tmux path needs `env -i`-style launch or `new-session -e` per variable, not `unset`.

### State B

`AgentEnv::for_spec(spec, launch_env) -> Vec<(OsString, OsString)>` in one place (crate chosen in
pass 2: lowest crate every site already depends on); each site `env_clear()`s and applies it.

## Testing Plan

A test per construction site plants `CLAUDE_CODE_CHILD_SESSION` and a `pass_env` variable in the
parent env and asserts the child sees only the latter (and the base). Unit tests on the constructor
and the schema validation.

## Acceptance Tests

| Test | File |
|---|---|
| `a_pass_env_entry_matching_tddy_fails_spec_loading_naming_the_spec` | `packages/tddy-coding-agent-specs/tests/pass_env_schema.rs` |
| `the_agent_env_is_the_base_plus_pass_env_plus_launch_env` | `packages/tddy-agent-backend/src/agent_env.rs` (`#[cfg(test)]`, crate per pass 2) |
| `a_headless_turn_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `an_acp_agent_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `a_pty_and_a_tmux_agent_see_no_unlisted_daemon_variable` | `packages/tddy-agent-launch/tests/agent_env_allow_list.rs` |
| `model_listing_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `every_builtin_spec_declares_its_provider_key_variables` | `packages/tddy-coding-agent-specs/tests/pass_env_schema.rs` |

## Responsibility

The agent process environment on the host: the allow-list, its spec field, and every host
construction site using it.

## Boundaries

- Does **not** change jailed launches (the runner already allow-lists).
- Does **not** change tool (action) or git process envs.
- Does **not** touch Codex OAuth env vars — Codex OAuth follow-up stack (they are launch-set, so they keep reaching Codex until that stack moves them).

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/session-pointer` | headless conversations carry no `TDDY_*`; pointer is the session channel | builds headless env without server env | re-add `TDDY_*` |
| `feature/agent-tools/split-remote-env` | split agents carry no `TDDY_REMOTE_*` | allow-list need not special-case them | change split wiring |

## Draft PR contract

The first push after this plan: the `pass_env` schema field and the shared constructor's signature,
plus the seven acceptance tests above, failing.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — the per-site tests assume `TDDY_*` delivery has already moved off the process (session-pointer, split-remote-env)
**Concurrent with:** none
**Blocks:** nothing

## Decisions & Trade-offs

- Base set approved by the developer (2026-11-06), `SSH_AUTH_SOCK` included.
- Complexity 5: the risk is an undeclared variable an agent needs; each built-in spec's `pass_env` is filled from what the agent's docs say it reads, and a missing one is a spec edit, not a code change.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

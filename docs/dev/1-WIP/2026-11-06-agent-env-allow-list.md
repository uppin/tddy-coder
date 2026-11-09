# Changeset: Agents start from an allow-listed environment

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Feature
**PR**: [#836](https://github.com/uppin/tddy-coder/pull/836) (`#agent-tools` 28/28)
**Complexity**: 5 — spec schema (`pass_env`) + a YAML base set + eight crates + every host launch path (headless, ACP, mint, listing, PTY, tmux, split PTY, supervisor relay); a missed variable breaks an agent at runtime. Pass 2 assessed it as 6 (eight crates, ten sites). Kept at 5 by the user's decision (2026-11-07).

## Initial Discovery

[2026-11-06-agent-env-allow-list-initial-discovery.md](./2026-11-06-agent-env-allow-list-initial-discovery.md)

## Prerequisites

No todo record. The rule ("agents start from a clean env, not the daemon's env minus `TDDY_*`")
was collected as a sweep item on 2026-11-06, after a desktop-dev run passed `CLAUDE_CODE_*` markers
to agents. No `Claimed by:` record exists in any touched package.

Recorded here for the sweep (not fixed by this node):
[2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md](../todo/2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md).

## Affected Packages

- **tddy-coding-agent-specs**:
  - `coding_agent.pass_env` (names and `PREFIX_*`); an entry admitting `TDDY_*` is a load error.
  - The built-in specs declare their provider keys.
  - The base set is YAML: built in as `specs/agent-env.yaml`, overridden whole by `<tddyhome>/tools/agent-env/base.yaml` (`AgentEnvBase`, loaded by `CodingAgentRegistry::load`).
  - `AgentEnvBase::agent_env(spec, parent, launch)` computes an agent's whole environment.
- **tddy-agent-backend**:
  - `agent_env`: `agent_child_env(base, parent, launch)`, `apply_agent_child_env(command, env)`, `matches_env_pattern`. The base set is a parameter, never a constant.
  - The headless turns (`turn_process.rs`), the model listing (`spec_headless.rs`, `HeadlessModelListing.env`), the ACP worker and the conversation-id mint start from exactly the env the daemon computed.
- **tddy-agent-launch**:
  - `HostAgent.env_base`.
  - The PTY launch (`spec_launch.rs`) gives the agent `env_base.agent_env(…)`.
  - The tmux pane keeps only the names of that env (`in_a_pane_with_only_the_agent_env`).
- **tddy-pty**: `PtySpawnSpec.env_base: PtyEnvBase { Inherited, Cleared }`.
- **tddy-cli-sessions**: `spawn_rendered_main` (agent mains) spawns on `Cleared`. User Bash terminals keep `Inherited`.
- **tddy-session-split**: the split agent's PTY (`spawn_split_agent_pty`, was `svc_spawn_split_agent.rs:348` inline) gets the registry's computed env.
- **tddy-supervisor**: a relayed coding agent gets exactly the daemon-computed env:
  - `policy::resolve_relayed_env`: no `allowed_env_keys`; loader keys are still refused.
  - `EnvironmentBase::Exactly`: no supervisor `PATH` or locale; the account's `HOME`/`USER`/`LOGNAME` only.
- Mechanical only (a field on `PtySpawnSpec` literals): **tddy-terminal-rpc** (passes `env_base` through), **tddy-coder** (`terminal_manager.rs`, `Inherited`).

## Related Feature Documentation

- PRD: [PRD-2026-11-06-agent-env-allow-list.md](../../ft/daemon/1-WIP/PRD-2026-11-06-agent-env-allow-list.md)

## Summary

The daemon computes every host agent child's whole env: the YAML base set's variables of the daemon, the spec's `pass_env` values, and what the launch sets on purpose. Every spawn site starts the child from exactly that env, with nothing inherited. An inherited `TDDY_*` can never pass.

## Scope

- [x] `pass_env` schema (names and `PREFIX_*`), `TDDY_*` refused at load
- [x] Built-in specs declare provider keys: claude/claude-acp `ANTHROPIC_API_KEY`, codex/codex-acp `OPENAI_API_KEY`, cursor `CURSOR_API_KEY`, opencode both. muse/warp/oz pass none (user, 2026-11-07).
- [x] Base set as YAML:
  - The built-in `specs/agent-env.yaml` holds the approved set (2026-11-06): `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `LC_*`, `TMPDIR`, `TZ`, `SSH_AUTH_SOCK`, `HTTP(S)_PROXY`/`NO_PROXY` (both cases), `XDG_*`.
  - An operator overrides it whole with `<tddyhome>/tools/agent-env/base.yaml`. It sits in a subdirectory because every `*.yaml` directly in `tools/` is a tool file.
  - `TDDY_*` in it is a load error, naming the file.
- [x] `AgentEnvBase::agent_env` computes the env in one place, over `tddy_agent_backend::agent_env::agent_child_env`, which takes the base as data.
- [x] Converted sites:
  - `turn_process`, model listing, ACP worker, conversation-id mint.
  - `spec_launch` (direct PTY and tmux pane).
  - `spawn_rendered_main` (`PtyEnvBase::Cleared`).
  - The split agent's PTY.
  - The supervisor relayed spawn: `resolve_relayed_env` + `EnvironmentBase::Exactly`.
- [x] The host registry's base set reaches every site. `HostCodingAgents` is built from a `ToolRegistry`, so green loads the override where the daemon builds it.
- [x] Deny-list helpers deleted at green: `strip_tddy_env`, `without_inherited_tddy_variables`, `in_a_pane_without_inherited_tddy` and its unit test (user, 2026-11-07).
- [x] The supervisor's `allowed_env_keys` no longer decides a **relayed coding agent's** env. This is approved by the user's answer of 2026-11-07. `allowed_env_keys` stays for `SpawnSession`/clone spawns; removing it there is not in scope.

## Technical Changes

### State A

See the Exploration 2 table "Agent-child env construction sites", corrected in pass 2:

- The PTY runtime (`tddy-pty`) inherits the whole daemon env.
- The split agent's PTY inherits the whole daemon env.
- The tmux pane inherits the tmux server's env.
- The supervisor's relayed spawn gates every key on `allowed_env_keys` and adds its own `PATH`/`LANG`/`TZ`.
- `tddy-spawn`'s spawner starts `tddy-coder`, not an agent.

### State B

- The daemon computes the agent's whole env with `AgentEnvBase::agent_env(spec, std::env::vars_os(), launch)`, where the base set is YAML data from the registry.
- Each spawn site applies exactly that env:
  - `Command::env_clear` + `envs` (`apply_agent_child_env`);
  - `PtyEnvBase::Cleared`;
  - the tmux pane unsetting every name the env does not set;
  - `EnvironmentBase::Exactly` in the supervisor.

## Testing Plan

- Each construction site plants `CLAUDE_CODE_CHILD_SESSION` and an inherited `TDDY_*` in the parent env. It asserts the child sees neither, and does see exactly what it was given.
- Unit tests cover `agent_child_env`, the pattern matcher, the base-set YAML (built in, override, `TDDY_*` refusal), `pass_env` validation, the PTY env base and the supervisor's relayed env.

## Acceptance Tests

| Test | File |
|---|---|
| `a_pass_env_entry_matching_tddy_fails_spec_loading_naming_the_spec` | `packages/tddy-coding-agent-specs/tests/pass_env_schema.rs` |
| `every_builtin_spec_declares_its_provider_key_variables` | `packages/tddy-coding-agent-specs/tests/pass_env_schema.rs` |
| `an_operator_override_of_the_base_set_is_honoured` | `packages/tddy-coding-agent-specs/tests/agent_env_base_load.rs` |
| `a_tddy_entry_in_the_base_set_fails_loading_naming_the_file` | `packages/tddy-coding-agent-specs/tests/agent_env_base_load.rs` |
| `an_agent_env_is_the_base_set_then_the_spec_pass_env_then_the_launch_env` | `packages/tddy-coding-agent-specs/tests/agent_env_base_load.rs` |
| `the_agent_env_is_the_base_plus_pass_env_plus_launch_env` | `packages/tddy-agent-backend/src/agent_env/agent_env_tests.rs` |
| `a_headless_turn_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `an_acp_agent_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `model_listing_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `a_conversation_id_mint_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-backend/tests/agent_env_allow_list.rs` |
| `a_pty_agent_sees_no_unlisted_daemon_variable` | `packages/tddy-agent-launch/tests/agent_env_allow_list.rs` |
| `a_pane_keeps_only_the_variables_of_the_agent_env` | `packages/tddy-agent-launch/src/spec_launch/pane_with_only_the_agent_env_tests.rs` |
| `a_child_on_a_cleared_base_sees_only_its_spec_env_and_the_terminal_variables` | `packages/tddy-pty/tests/pty_env_base.rs` |
| `an_agent_main_spawned_through_spawn_rendered_main_sees_no_unlisted_daemon_variable` | `packages/tddy-cli-sessions/tests/agent_env_allow_list.rs` |
| `a_user_terminal_still_inherits_the_daemon_environment` (guard) | `packages/tddy-cli-sessions/tests/agent_env_allow_list.rs` |
| `the_split_agents_pty_sees_no_unlisted_daemon_variable` | `packages/tddy-session-split/src/svc_spawn_split_agent/split_agent_pty_tests.rs` |
| `a_relayed_agent_env_passes_whole_with_no_supervisor_key_list` | `packages/tddy-supervisor/src/policy.rs` (`#[cfg(test)]`) |
| `a_relayed_agent_starts_from_exactly_the_env_the_daemon_computed` | `packages/tddy-supervisor/src/spawn_broker.rs` (`#[cfg(test)]`) |

**Platform:** the supervisor tests are pure unit tests: the env gate, and the built env read back without a fork. They run on macOS. The end-to-end relayed spawn (a live child as another user, bytes relayed) is Linux-only, like every spawning test in `tests/supervisor_relay.rs`. Green adds a Linux case there asserting the child's env.

## Responsibility

The agent process environment on the host:

- the allow-list: the YAML base set and the spec's `pass_env`;
- the one place the env is computed;
- every host construction site that uses it, including the supervisor's relayed spawn.

## Boundaries

- Does **not** change jailed launches (the runner already allow-lists).
- Does **not** change tool (action) or git process envs, user Bash terminals, or the supervisor's `SpawnSession`/clone/sandbox env gates.
- Does **not** touch `tddy-spawn`. Its spawner starts `tddy-coder`, not an agent (dropped from the plan, user 2026-11-07).
- Does **not** move Codex OAuth's env:
  - The launch-set `TDDY_CODEX_OAUTH_OUT` keeps reaching Codex. It is a documented exception that expires with the Codex OAuth follow-up stack.
  - `codex login` (`codex.rs:53`) is out of scope.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/session-pointer` | headless conversations carry no `TDDY_*`; the pointer is the session channel | builds the headless env without the server env | re-add `TDDY_*` |
| `feature/agent-tools/split-remote-env` | split agents carry no `TDDY_REMOTE_*` | the allow-list need not special-case them | change split wiring |
| `feature/agent-tools/agent-tool-socket-per-os-user` | `agent_tool_socket_of(os_user)` for the split agent's hooks | nothing env-related | touch socket placement |

No red test drives a parent's `TODO(...)`.

## Draft PR contract

The first push after the plan: the owned surface, with `TODO(env-allow-list)` bodies, plus the failing tests above.

- `agent_env` (backend).
- `pass_env`, `AgentEnvBase` with its YAML, and the registry's `agent_env_base` (specs).
- `HostAgent.env_base` and `in_a_pane_with_only_the_agent_env` (agent-launch).
- `PtyEnvBase` (pty).
- `spawn_tool_on` (cli-sessions).
- `spawn_split_agent_pty` (session-split).
- `resolve_relayed_env` and `EnvironmentBase::Exactly` (supervisor).

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. The per-site tests assume `TDDY_*` delivery has already moved off the process (session-pointer, split-remote-env).
**Concurrent with:** none
**Blocks:** nothing

## Decisions & Trade-offs

- Base set approved by the developer (2026-11-06), `SSH_AUTH_SOCK` included.
- Complexity 5: the risk is an undeclared variable an agent needs. Each built-in spec's `pass_env` is filled from what the agent's docs say it reads; a missing variable is a YAML edit, not a code change.
- **Tests (user, 2026-11-07, via orchestrator AskUserQuestion):** "Approve + OAuth exception (Recommended)".
  - An inherited `TDDY_*` is always dropped.
  - A `TDDY_*` the launch sets on purpose (today only `TDDY_CODEX_OAUTH_OUT`) is kept. This is a documented exception, named in the `agent_env` module doc, that expires with the Codex OAuth stack.
  - `codex login` (`codex.rs:53`) stays out of scope.
- **PTY scope (user, 2026-11-07, via orchestrator AskUserQuestion):** "Widen n28 (Recommended)".
  - tddy-pty, tddy-cli-sessions and tddy-session-split are added.
  - `spawn_rendered_main` clears the env for agent mains; user Bash terminals keep inheriting.
  - The split agent's PTY is in scope.
  - Complexity stays 5; pass 2 assessed 6.
  - Note: adding a field to `PtySpawnSpec` also needed mechanical literal edits in tddy-terminal-rpc and tddy-coder.
- **Drop tddy-spawn (user, 2026-11-07, via orchestrator AskUserQuestion):** removed from the plan and the title.
- **Delete the deny-list (user, 2026-11-07, via orchestrator AskUserQuestion):** `strip_tddy_env`, `without_inherited_tddy_variables`, `in_a_pane_without_inherited_tddy` and its unit test are deleted at green.
- **muse/warp/oz pass no keys (user, 2026-11-07, via orchestrator AskUserQuestion).**
- **Supervisor and YAML allow-lists (user, 2026-11-07, via orchestrator AskUserQuestion), verbatim:** "supervisor shouldn't be aware of any env or coding agent specifics. It's job is to manage tddy-daemon on the correct OS user, manage cgroups etc. It shouldn't inherit config that normally should belong to tddy-daemon. Also, any coding specific stuff should be configurable by YAML, so also allow-lists."
  - Reading:
    - The daemon computes the agent's whole env: base + `pass_env` + launch, minus inherited `TDDY_*`, with the OAuth exception.
    - A relayed launch sends that env as is.
    - The supervisor applies no agent-specific env filtering: `allowed_env_keys` and `ChildEnvironment`'s `PATH`/locale do not decide a relayed agent's env.
    - The supervisor keeps its own concerns: the OS user (`HOME`/`USER`/`LOGNAME` of the account), cgroups, its own environment for the daemon it manages, and refusing loader variables (`LD_PRELOAD`, …).
    - The base allow-list is YAML, not a Rust constant: a built-in default plus an operator override in the tools dir. `TDDY_*` in it is a load error.
  - Choice: one registry-level base file, `<tddyhome>/tools/agent-env/base.yaml`, rather than a per-spec `base_env`. One set to audit; a spec widens it only through its own `pass_env`.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (user, 2026-11-07, via orchestrator AskUserQuestion)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass (scoped; one pre-existing failure below)
- [x] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
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
- Stack gate: base `feature/agent-tools/acp-subagent-leftovers` is an ancestor of HEAD; `origin/<base>..HEAD` holds only this PR's commits (plan + red). Dependency gate: parent surfaces present on the branch; no parent-owned symbol implemented here.
- Responsibility delivered: every `TODO(env-allow-list)` body implemented; deny-list helpers (`strip_tddy_env`, `without_inherited_tddy_variables`, `in_a_pane_without_inherited_tddy` + its unit test) deleted. Boundaries held: no jailed launch, tool/git env, user terminal, `SpawnSession`/clone gate, `tddy-spawn`, or `codex login` change.
- Scoped tests (2026-11-09): `./test -p tddy-agent-backend`, `-p tddy-coding-agent-specs`, `-p tddy-pty`, `-p tddy-cli-sessions`, `-p tddy-supervisor`, `-p tddy-agent-launch`, `-p tddy-session-split`, `-p tddy-coder` green. `tddy-session-lifecycle`: green except the pre-existing `claude_cli_from_spec_acceptance::an_unsandboxed_claude_cli_start…` path-canonicalisation failure, which fails before this node and is not this PR's.
- One existing assertion adjusted: `claude_cli_tmux_session_acceptance::the_pane_environment_file_is_gone…` now checks no `CLAUDE_CODE_NO_FLICKER=` assignment is on tmux's argv (names only, never values — the sanctioned pane design), instead of the variable's name.
- Clippy: `./dev cargo clippy -p <touched> --all-targets -- -D warnings` clean; `cargo fmt` applied.
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

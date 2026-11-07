# Changeset: ACP subagent leftovers

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 5 — six crates (coding-subagent, coding-agent-specs incl. a new spec field, session-agents, session-lifecycle, daemon-rpc, session-split fixture), every agent table re-pointed onto the registry, the hooks adapters honoured by the daemon launcher. Planner's assessment was 6 (above the scale's top); recorded as 5, the scale's maximum.
**PR**: [#835](https://github.com/uppin/tddy-coder/pull/835)

## Initial Discovery

[2026-11-06-acp-subagent-leftovers-initial-discovery.md](./2026-11-06-acp-subagent-leftovers-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md) | ✅ RESOLVED HERE (launcher half fixed by #825; `tddy-coding-subagent` remainder here) |
| [2026-11-04-opencode-acp-route-file-has-no-session-owner.md](../todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md) | **Stays open** — owned by the follow-up route-file node (user, 2026-11-07); this PR keeps the inline route |
| `2026-11-05-model-listing-and-bespoke-acp-backends-remain.md` | Already gone at the base (deleted by #828); #832 handed over no reachable backend, so nothing is re-pointed here |

## Affected Packages

- **tddy-coding-subagent** — `session_meta_for` loses its default (a required method); the agent tables go (`confinement_for`, `refuse_unless_confinable`, `withdrawal_session_meta`, `opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`, `CLAUDE_ACP`, `CODEX_ACP`, `OPENCODE`, `ACP_HARNESSES`, `HEADLESS_HARNESSES`); transport, permission policy, catalog filtering and confinement read the spec / registry (`transport_of_spec`, `permission_policy_of_spec`, `transport_and_policy_wire_of_spec`, `coding_agent_defs_from`, `refuse_unless_registry_confinable`); `CodingAgentSession::start`/`start_over` take the launch's `PreparedAcpHooks`; `CodingAgentLauncher::launch_agent` → `LaunchedAgent`. `OPENCODE_CONFIG_CONTENT` stays (D1).
- **tddy-coding-agent-specs** — new `coding_agent.headless.permission_policy` (`asks_caller` | `{ fixed: <text> }`, `HeadlessPermissionPolicy`), declared in `claude`, `cursor`, `codex`, `oz.yaml`. No JSON schema exists for agent specs: the Rust struct is the schema, and `tddy-workflow-recipes/generated/schema-manifest.json` is unaffected.
- **tddy-session-lifecycle** — `DaemonCodingAgentLauncher::launch_agent` prepares every declared `acp.hooks_adapter` (`codex-acp-login`); `coding_agent_launcher/acp.rs` keys route `-c` args on the spec's `route.shape: toml_args`, not `CODEX_ACP`, and drops its `OPENCODE` constant; `handler_state` hands the roster the host registry.
- **tddy-session-agents** — `AgentRoster.coding_agents`; `coding_agent_defs_for` and `refuse_unconfinable_harness` take the registry; `start_acp_session` opens through `launch_agent`.
- **tddy-daemon-rpc** — `coding_agent_rows` / `coding_agent_info` read the registry for the row's transport and policy.
- **tddy-session-split** — test fixture only (`split_room_hosting_port.rs` builds the new roster field).

## Related Feature Documentation

None — no user-visible change. No PRD.

## Summary

`tddy-coding-subagent` stops naming agents: its withdrawal, transport, permission-policy, catalog and
confinement answers come from the specs in the host registry, and an ACP session runs with — and holds
for its lifetime — the hooks its agent's declared adapter prepared.

## Scope

- [ ] `session_meta_for` has no default (required method; the 13 test launcher impls get one-liners)
- [ ] the agent tables in `confinement.rs`, `catalog.rs`, `transports.rs` deleted; consumers read the spec / registry
- [ ] transport and permission policy from the spec (`headless.permission_policy`); catalog filtering and attach-time confinement from the registry (session-agents, daemon-rpc)
- [ ] `codex_acp_route_args` applied on `route.shape: toml_args`, not on the `codex-acp` name
- [ ] `CodingAgentSession::start` runs with and holds the launch's hooks and brackets its open; `launch_agent` honours every declared `acp.hooks_adapter`
- [x] ~~reachable ACP backend leftovers handed over by `feature/agent-tools/dead-launch-code`~~ — moot: #832 handed over none
- [x] ~~OpenCode route file owned by the session~~ — moved to the follow-up route-file node (D1)

## Technical Changes

### State A

Exploration 3 items 6, 7, 10, re-read against the landed #829/#832 (pass 2): `tddy-coding-subagent`
`launcher.rs` `session_meta_for` default (`TODO(acp-agents)`); `confinement.rs`, `catalog.rs:9-15,61`,
`transports.rs:5-75` (28 agent-naming lines); `session.rs` `start`/`start_over` with `NoHooks`;
`tddy-session-lifecycle/src/coding_agent_launcher/acp.rs:33-68` (#829 split the launcher: inline
`OPENCODE_CONFIG_CONTENT`, `harness == CODEX_ACP` route args, no hooks adapter prepared);
`tddy-session-agents/src/coding_agent.rs:28,84-111` and `tddy-daemon-rpc/src/catalog/subagent_row.rs`
(name-based catalog and confinement). The spec-driven withdrawal itself (`acp_withdrawal`,
`confinement_of_spec`) was already landed by #800.

### State B

Agent-specific withdrawal, transport, permission-policy and confinement data live in the specs;
`tddy-coding-subagent` has no agent name in production code (`tests/no_agent_names.rs`), and
the daemon launcher prepares whatever hooks adapter an agent declares.

## Testing Plan

Integration via `tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` (declared hooks
adapter prepared, route args from the spec's shape); registry-driven unit tests in
`tddy-coding-subagent/tests/registry_driven_agents.rs`; session hooks lifetime in
`tddy-coding-subagent/tests/session_hooks_lifetime.rs`; a source scan.

## Acceptance Tests

| Test | File | Kind |
|---|---|---|
| `coding_subagent_names_no_agent` (source scan) | `packages/tddy-coding-subagent/tests/no_agent_names.rs` | 🔴 red |
| `a_codex_acp_launch_runs_codex_login_and_retries_once_on_an_auth_like_session_error` | `packages/tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` | 🔴 red (D4) |
| `a_route_reaches_an_acp_agent_as_overrides_only_when_its_spec_declares_toml_args` | same | 🔴 red |
| `a_managed_session_refuses_an_acp_agent_whose_hosts_spec_cannot_withdraw_its_tools` | `packages/tddy-coding-subagent/tests/registry_driven_agents.rs` | 🔴 red |
| `a_headless_agents_permission_policy_is_the_text_its_spec_declares` | same | 🔴 red |
| ~~`an_opencode_acp_session_owns_its_route_file_for_its_lifetime`~~ | — | moved to the route-file node (D1) |
| ~~`withdrawal_comes_from_the_spec_acp_slot`~~ | `packages/tddy-coding-agent-specs/tests/acp_slots_unit.rs` | already satisfied at the base; its existing tests are the guard |

Unit/integration (6f): `session_hooks_lifetime.rs` (`a_started_session_holds_its_launch_hooks_while_it_lives` 🔴,
`a_session_brackets_its_open_with_its_launch_hooks_open_scope` 🔴, `a_dropped_session_lets_its_launch_hooks_go` guard);
`registry_driven_agents.rs` (`a_dropped_in_agent_with_a_headless_section_is_spoken_to_headless` 🔴,
`a_profile_on_a_dropped_in_headless_agent_is_offered` 🔴, `an_allowed_acp_agent_runs_the_default_model_its_hosts_spec_declares` 🔴,
plus five guards pinning today's built-in answers); `acp_slots_unit.rs`
`every_built_in_headless_spec_declares_who_answers_its_permission_requests` (guard);
`acp_agents_from_spec_acceptance.rs` `a_codex_acp_route_still_reaches_it_as_one_model_override_under_its_built_in_spec`
and `an_agent_whose_spec_declares_no_hooks_adapter_is_launched_with_the_command_it_plans` (guards).

## Responsibility

The agent-specific tables left in `tddy-coding-subagent` and their consumers in `tddy-session-agents`,
`tddy-daemon-rpc` and `tddy-session-lifecycle`; the hooks a daemon-launched ACP agent's declared
adapter prepares, held by its session.

## Boundaries

- Does **not** delete unreachable backends — `feature/agent-tools/dead-launch-code`.
- Does **not** touch the headless conversation pointer — `feature/agent-tools/session-pointer`.
- Does **not** move the OpenCode route out of `OPENCODE_CONFIG_CONTENT` — the follow-up route-file node (D1).

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/session-pointer` | `coding_agent_launcher.rs` without `forwards_server_env` | edits the same file after it | re-add server env forwarding |
| `feature/agent-tools/dead-launch-code` | unreachable ACP backends deleted; reachable list | re-points the reachable ones | delete code itself |

## Draft PR contract

Published in commit 2, bodies marked `TODO(acp-subagent-leftovers)`:

- `tddy_coding_subagent::{LaunchedAgent, no_launch_hooks}`; `CodingAgentLauncher::launch_agent`
  (default: `launch_transport` + `no_launch_hooks()`); `CodingAgentSession::start(command, open, model,
  hooks: PreparedAcpHooks)` and `start_over(stream, open, model, hooks)`.
- `transport_of_spec`, `permission_policy_of_spec`, `transport_and_policy_wire_of_spec`,
  `coding_agent_defs_from(registry, profiles, allowed)`, `refuse_unless_registry_confinable(registry,
  harness, managed)` — stubs delegate to the name tables.
- `tddy_coding_agent_specs::{HeadlessPermissionPolicy, HeadlessSection::permission_policy}` and the four
  YAML declarations (implemented: data and parsing).
- `DaemonCodingAgentLauncher::launch_agent` override (stub: no hooks).
- `AgentRoster.coding_agents: Arc<CodingAgentRegistry>`; `coding_agent_defs_for(…, coding_agents)`,
  `refuse_unconfinable_harness(…, coding_agents)`; daemon-rpc `coding_agent_rows(…, coding_agents)`,
  `coding_agent_info(…, coding_agents)` — the new parameter is ignored until green.
- `session_meta_for` keeps its default until green removes it (D3); `session_agent_roster.rs`'s
  snapshot wire (`transport_and_policy_wire`) is re-pointed at green too.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** yes — #829 and #832 are implemented below it; no test reaches a parent's TODO
**Concurrent with:** `feature/agent-tools/split-remote-env`, `feature/agent-tools/agent-tool-socket-per-user`
**Blocks:** nothing

## Decisions & Trade-offs

Pass-2 answers — user, 2026-11-07, via orchestrator AskUserQuestion:

- **D1 — "Own it, separate node".** A daemon-written route file (daemon temp dir, 0700/0600) cannot be read
  by an agent started as the session's OS user (`setpriv` or supervisor relay). The route file, owned by
  the session user (chown when privileged; the supervisor writes it for relayed launches), becomes its own
  follow-up stack node. Here: the inline `OPENCODE_CONFIG_CONTENT` route stays, the route-file reds and
  inline-route test rewrites are out, `opencode_route_config` is kept, and the todo record stays open.
  The hooks parameter / `LaunchedAgent` stays.
- **D2 — "Full".** Full State B in this node: a new `headless.permission_policy` spec field, transport and
  permission policy from the spec, catalog filtering and attach-time confinement from the registry,
  `tddy-session-agents` and `tddy-daemon-rpc` in scope.
- **D3 — "Required method (Recommended)".** `session_meta_for` has no default; the 13 test launcher impls
  get one-liners at green.
- **D4 — "Yes, honour all (Recommended)".** `launch_agent` prepares every declared `acp.hooks_adapter`, so
  daemon-launched `codex-acp` gains the `codex login` retry-once hook.
- **Deletions approved** (user, 2026-11-07): `confinement_for`, `refuse_unless_confinable`,
  `withdrawal_session_meta`, `opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`
  and their inline tests; `CLAUDE_ACP`, `CODEX_ACP`, `OPENCODE`, `ACP_HARNESSES`, `HEADLESS_HARNESSES`.
  `OPENCODE_CONFIG_CONTENT` stays (D1).

Open for green (found writing the tests):

- `codex-acp-login`'s `prepare` already appends `codex_acp_route_args` on a route; with D4 the launcher's
  own `toml_args` append must not double it (pinned by
  `a_codex_acp_route_still_reaches_it_as_one_model_override_under_its_built_in_spec`), and the adapter's
  append must respect `route.shape` too (pinned by the `shape: env` override red).
- Which session directory `CodingAgentSession` hands `opens.begin_open` (the login writes its authorize
  URL there): the conversation's headless directory or the worktree — pick one and document it.
- A headless agent stays refused in a managed session under `refuse_unless_registry_confinable` (pinned
  by a guard) — the spec-based answer must not silently admit headless `claude` because its spec
  declares a `--disallowedTools` flag.
- `headless.permission_policy` is optional; a headless agent declaring none has no policy and is not
  offered — not a default.

Resolved at green (2026-11-07):

1. **One `-c` override.** `acp_plan_from_spec` renders the route's `-c` args when the spec's
   `route.shape` is `toml_args` (not on `CODEX_ACP`) and OpenCode's inline config when it is
   `json_config_file`. `launch_agent` hands the declared adapter **no route** (`route: None`): the
   plan already rendered it, so `codex-acp-login`'s own `codex_acp_route_args` append (and
   `opencode-route-file`'s file write, which D1 forbids here) never run. The adapter itself
   (`tddy-agent-backend`, out of this node's packages) is unchanged; it stays correct for the
   `tddy-coder` path, which hands it the route and no plan-rendered args.
2. **Session directory.** `CodingAgentSession` hands `begin_open` the open's `cwd` — the worktree
   the agent works in (for a clone placement, the conversation's own). The ACP session has no
   directory of its own and a headless conversation directory exists only for headless; the cost is
   that a `codex login` writes its authorize-URL file into that worktree (finding, not fixed here).
3. **Headless `claude` stays refused.** `refuse_unless_registry_confinable` goes through
   `confinement_of_spec`, which needs an `acp` section; a headless spec's `--disallowedTools` flag
   is not an ACP withdrawal. Pinned by `a_managed_session_still_refuses_a_headless_agent_…`.
4. **Snapshot wire.** `SessionAgentRosterStore::new` takes the host's coding-agent registry
   (`SnapshotSources::coding_agents`), and `DaemonSessionHost::with_coding_agents` rebuilds the store
   over it; the roster entry's `transport` / `permission_policy` are read from the spec.
5. **D1 over D4 (orchestrator).** `specs/opencode.yaml` declares `acp.hooks_adapter:
   opencode-route-file`; preparing it with the route would point `OPENCODE_CONFIG` at a 0600 file in
   the daemon's temp dir, unreadable by an agent running as another OS user. The launcher names no
   agent: it prepares every declared adapter but hands each **no route** (point 1), so
   `opencode-route-file` writes nothing and sets no variable, and the route stays inline. A
   capability flag on the adapter was not added (`tddy-agent-backend` is outside this node). Guard:
   `a_routed_opencode_launch_keeps_its_route_inline_and_names_no_route_file`. The follow-up
   route-file node hands the adapter the route.
6. **Name-keyed wrappers kept.** `transport_of`, `permission_policy_of`, `transport_and_policy_wire`,
   `coding_agent_defs` answer from the built-in registry for a caller with no host; no production
   code calls them (deleting them was not in the approved list).
7. **Signature changes forced by the above:** `coding_agent_defs_in(registry, dir, allowed)`,
   `start_coding_agent_session(launcher, launch, coding_agents)`; one contract test call site
   (`headless_coding_agent_opens_as_a_headless_conversation.rs`) and the `catalog_acceptance.rs`
   `confinement_for` tests (re-pointed at `refuse_unless_registry_confinable`) were edited.

- The planning-time line said this node waits on bounded-waits for a shared launcher file; discovery shows the shared file is `coding_agent_launcher.rs`, which session-pointer edits — the real edges are session-pointer and dead-launch-code.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (user, 2026-11-07, via orchestrator: D1–D4)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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

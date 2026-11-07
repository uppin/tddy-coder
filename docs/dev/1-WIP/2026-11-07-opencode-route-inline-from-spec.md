# Changeset: A daemon ACP launch hands its route inline, in the variable the spec names

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 3 — a new spec field and its load validation, plus the daemon ACP planner and `launch_agent`, with integration tests through `tddy-acp-stub`. The scoring rule reaches 4 because two crates are touched (specs and session-lifecycle); it is kept at 3 as planned, because the specs half is one optional field.

## Initial Discovery

[2026-11-07-opencode-route-inline-from-spec-initial-discovery.md](./2026-11-07-opencode-route-inline-from-spec-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-opencode-acp-route-file-has-no-session-owner.md](../todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md) | ✅ RESOLVED HERE, closed as **"file not needed"** (user, 2026-11-07); delete at wrap |

No `Claimed by:` record exists in the touched packages (`tddy-coding-agent-specs`,
`tddy-session-lifecycle`).

## Affected Packages

- **tddy-coding-agent-specs**:
  - `AcpSection.route_inline_env: Option<String>` (`src/spec.rs:480-504`).
  - Load errors for that field: the agent's `route.shape` is not `json_config_file`, or the name matches `TDDY_*`.
  - `specs/opencode.yaml` declares `route_inline_env: OPENCODE_CONFIG_CONTENT`, and its comments (:7-8, :70-76) say which path writes a file and which goes inline.
- **tddy-session-lifecycle**:
  - `coding_agent_launcher/acp.rs`:
    - `acp_plan_from_spec` merges the route into the spec's `route_inline_env`, which replaces the `harness == OPENCODE` branch (:60-67) and its `TODO(acp-agents)` (:33-35).
    - `opencode_route_config` (:232-246) is renamed `route_inline_config` and no longer names an agent.
  - `coding_agent_launcher.rs`: `launch_agent` (:376-391, #835's override) hands no route to a hooks adapter when the launch's route went inline, so `opencode-route-file` writes nothing on the daemon path. Its doc (:376-378) and the module doc (:13) are updated.
- **Docs only**: `supervisor.yaml.production:115-116` (stale `allowed_env_keys` note).

## Related Feature Documentation

- PRD: [PRD-2026-11-07-opencode-route-inline-from-spec.md](../../ft/daemon/1-WIP/PRD-2026-11-07-opencode-route-inline-from-spec.md)

## Summary

The spec declares where a daemon ACP launch puts its route inline (`coding_agent.acp.route_inline_env`).
The daemon's ACP planner reads that field rather than the agent's name, and it writes no route file.
The `tddy-coder` ACP path keeps its `opencode-route-file` adapter.

## Scope

- [ ] `coding_agent.acp.route_inline_env` field, with its load validation
- [ ] `opencode.yaml` declares it
- [ ] The daemon planner merges the route by the field, not the name. `harness == OPENCODE` and `TODO(acp-agents)` are gone.
- [ ] The daemon's `launch_agent` writes no route file for an inline-route launch
- [ ] `tddy-coder`'s ACP path is unchanged (guard)
- [ ] `supervisor.yaml.production:115-116` and `docs/ft/supervisor/tddy-supervisor.md:92` no longer tell operators to list `OPENCODE_CONFIG_CONTENT` in `allowed_env_keys`. Do this only if #836's green has not already.
- [ ] Wrap: delete `docs/dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md`, recording "file not needed" and its three reasons in the change-history entry

## Technical Changes

### State A

Read at #836's tip (`ddef5cc6b`). #835's green was still running, so its tip is the contract commit
`df5574f52`.

- `acp.rs:60-67`: `if harness == OPENCODE { merge_inline_config(&mut command, OPENCODE_CONFIG_CONTENT, …) }`.
  The `OPENCODE` constant comes from `tddy-coding-subagent`, and #835 deletes it.
- `opencode.yaml:77-83` declares `hooks_adapter: opencode-route-file` and
  `tool_deny: { config_env: OPENCODE_CONFIG_CONTENT }`. `route.shape: json_config_file` is at :62-68.
- `OpenCodeRouteFile::prepare` (`tddy-agent-backend/src/adapters/opencode_route_file.rs:40-65`)
  works like this:
  - It writes the route's files into a fresh `0700` temp directory as `0600` files.
  - It sets `OPENCODE_CONFIG` to the first file.
  - With no route, or a route with no files, it changes nothing.
- #835's D4 makes the daemon's `launch_agent` prepare every declared `acp.hooks_adapter`. Today the
  contract stub at `coding_agent_launcher.rs:383-390` prepares none.
- The route file carries no key. `opencode_config` writes `"apiKey": "{env:<key_env>}"`
  (`provider_route.rs:146-180`).

### State B

- `acp_plan_from_spec` behaves like this on a routed launch:
  - When `agent.spec` declares `acp.route_inline_env = V`, the route's first file is parsed as JSON
    and merged into `V`. A file that is not JSON refuses the launch, as today.
  - Without the field, the planner adds no inline route.
  - No agent name is compared.
- The daemon's `launch_agent` hands its hooks adapter no route for a launch whose route went inline.
  So `opencode-route-file` writes nothing and sets no `OPENCODE_CONFIG`.
- The withdrawal `config_env` merge (`acp.rs:69-76`) is unchanged and still lands over the route.

## Testing Plan

- Integration tests in `tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs`. A wrapper
  `bin/<agent>` records its env and then execs `tddy-acp-stub`; the harness already records
  `OPENCODE_CONFIG_CONTENT` (:79-118).
- Load-error tests in the specs crate.
- The existing `coding_agent_launcher.rs` unit tests that pin the inline merge (:643-822) are
  re-keyed off the `OPENCODE` constant, which #835 deletes.

## Acceptance Tests

| Test | File | Kind |
|---|---|---|
| `a_routed_opencode_acp_launch_carries_its_route_inline_in_the_variable_its_spec_names` | `packages/tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` | guard (today's name branch already passes it) |
| `a_drop_in_acp_agent_declaring_route_inline_env_gets_its_route_inline_whatever_its_id` | same | 🔴 red |
| `a_daemon_acp_launch_writes_no_route_file_and_sets_no_opencode_config` | same | 🔴 red once #835's D4 prepares `opencode-route-file`; otherwise a guard |
| `a_managed_launchs_withdrawal_still_merges_over_the_inline_route` | same | guard |
| `a_route_inline_env_on_an_agent_whose_route_is_not_a_json_config_file_fails_loading_naming_the_spec` | `packages/tddy-coding-agent-specs/tests/` (pass 2 picks the file) | 🔴 red |
| `a_route_inline_env_naming_a_tddy_variable_fails_loading_naming_the_spec` | same | 🔴 red |
| `an_opencode_route_carries_the_config_the_key_and_the_model_argument` (existing) | `packages/tddy-agent-backend/tests/opencode_route_acceptance.rs:100` | guard: the `tddy-coder` path keeps its file |

## Responsibility

How a daemon ACP launch hands its provider route to an agent that takes inline config:
- the `route_inline_env` spec field;
- the daemon planner reading it;
- the daemon's `launch_agent` preparing no route file.

## Boundaries

- Does **not** change the `tddy-coder` ACP path or delete the `opencode-route-file` adapter.
- Does **not** change OpenCode's PTY session (`opencode-cli`, `OPENCODE_CONFIG` → session dir).
- Does **not** change `codex-acp` route args or `route.shape: toml_args` (#835).
- Does **not** touch the supervisor's env handling (#836).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/acp-subagent-leftovers` (#835, `#agent-tools` 27) | `CodingAgentLauncher::launch_agent` → `LaunchedAgent`; the daemon's override preparing declared hooks adapters (D4); `OPENCODE` deleted from `tddy-coding-subagent`; route args keyed on `route.shape` | keys the inline route on the spec; makes the daemon override hand no route to an inline agent's adapter | add `LaunchedAgent`, change hook preparation for other adapters, or re-add an agent-name constant |
| `feature/agent-tools/env-allow-list` (#836, `#agent-tools` 28) | the agent env from base + `pass_env` + launch; a relayed agent's env is not gated by `allowed_env_keys` | the inline variable is launch-set, so it reaches the agent without a `pass_env` entry | change `agent_env` or the supervisor's relayed env |

**Hard dependency:** #835's green. Until then `launch_agent` is a stub and `OPENCODE` still exists,
so pass 2 writes against #835's contract surface.

## Draft PR contract

The first push after this plan:
- the owned surface: `AcpSection.route_inline_env`, its loader check (body `TODO(opencode-route-inline)`), and the `opencode.yaml` declaration;
- the failing tests above.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. Its daemon-path tests exercise #835's `launch_agent` behaviour (D4 adapter preparation), which #835's green delivers.
**Concurrent with:** `feature/agent-tools/user-mapping-in-supervisor` (`#agent-tools` 31)
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    #835 → this node      #834 → #agent-tools 31      #836 ⇢ #agent-tools 31 (shared supervisor files)

## Decisions & Trade-offs

- **Inline, declared in YAML (user, 2026-11-07).** This reverses #835's D1 plan of a route file
  owned by the session user. The file is not needed:
  - no secret is in it;
  - inline outranks the worktree's `opencode.json`, while `OPENCODE_CONFIG` ranks below it, so a
    repository could redirect `baseURL` and harvest the routed key;
  - no directory owned by the session user exists.
- **The spec has two route mechanisms.** `hooks_adapter: opencode-route-file` is what `tddy-coder`
  uses, and `route_inline_env` is what the daemon uses. The daemon ignores a route-file adapter's
  route when the field is set.

  *Challenge:* reason 2 applies to the `tddy-coder` ACP path too, and to the PTY `opencode-cli`
  path. Both name their file with `OPENCODE_CONFIG`, so a repository's `opencode.json` outranks
  both. Moving `tddy-coder` to the inline route as well would delete the adapter and the second
  mechanism. That was not asked for here and is left for the user to decide; if accepted, it is a
  todo record at wrap.
- **The field is checked against the route shape at load.** A `toml_args` agent has no file to
  inline, so declaring the field there is a spec error, not a silent no-op.

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

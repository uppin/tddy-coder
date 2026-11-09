# Changeset: An operator start refuses peer-held parts, and a refused delete leaves the session intact

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — three crates own symbols (`tddy-agent-launch`, `tddy-session-split`, `tddy-session-agents`), the teardown's ordering semantics change, and the delete test is an integration test over a real tmux server (re-scored in pass 2; it was 2)

**PR**: [#858](https://github.com/uppin/tddy-coder/pull/858) (`#agent-tools` 30/32), base `feature/agent-tools/opencode-route-inline`

## Initial Discovery

[2026-11-07-operator-start-refuses-peer-held-parts-initial-discovery.md](./2026-11-07-operator-start-refuses-peer-held-parts-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-07-operator-start-undo-of-peer-held-parts.md](../todo/2026-11-07-operator-start-undo-of-peer-held-parts.md) | ✅ RESOLVED HERE, closed as **unreachable** (user, 2026-11-07): an operator start can no longer create a peer-held part, so its undo never needs a peer delete. Delete at wrap (deletion approved by the user's decision to close it here). |

No `Claimed by:` record exists for the touched files. `tddy-agent-launch/docs/code-issues/` holds
complexity records for `start_session_core` and other starts. None names `operator_start.rs`,
`start_task.rs` or `svc_signal_delete_session.rs`.

## Affected Packages

- **tddy-agent-launch**:
  - `operator_start.rs`: `start_session_for_os_user` (:26-57) refuses a request that would create a peer-held part, through `refuse_peer_held_parts`. The refusal is `INVALID_ARGUMENT` naming the field (user, 2026-11-07), made before the harness profile, the placement or any step runs. The fields are:
    - `codebase_daemon_instance_id`;
    - `sandboxed_codebase`;
    - `specialized_agents`;
    - `clone_placed_agents`.
  - `start_task.rs`: `TODO(operator-start-undo)` (:148-151) goes. The `undo_a_timed_out_start` doc (:139-154) says an operator start has no peer-held part to undo.
  - `svc_signal_delete_session.rs`: `delete_session_as` (:90-179) runs `refuse_a_delete_a_peer_would_refuse` **before** it kills the tmux session (:109-114) or removes the session pointer (:117-124), so a refused delete leaves the session and its agent running.
- **tddy-session-split**: `SplitSessions::refuse_an_unreachable_paired_codebase`, beside `delete_paired_codebase_session` (`svc_paired_codebase_teardown.rs:78`). Read-only: it refuses, with the teardown's own `FAILED_PRECONDITION`, a session paired with a codebase half on **another** daemon while this daemon has no common room configured or joined.
- **tddy-session-agents**: `AgentRoster::refuse_an_unreachable_clone_host`, beside `tear_down_every_agent_clone` (`svc_provision_agent_clone.rs:253-268`). Read-only: it refuses a session with a recorded clone while no common room is configured or joined.
- Pass 2 put the two pre-checks beside the teardowns they predict, not in `tddy-agent-launch`: the rule (a jailed codebase on this daemon needs no room; a cross-host half or a clone does) is the teardown's own, and a copy in another crate would drift from it.
- **tddy-session-lifecycle**: tests only. `DaemonSessionHost` builds the `LaunchSessions` both tests need.

## Related Feature Documentation

None. This is an internal fix with no new user-visible behaviour; an operator (Telegram) start never
sets these fields (`operator_session_starter.rs:55-80`), and the operator port's input,
`OperatorSessionStart` (`tddy-daemon-extensions/src/session_starter.rs:23-36`), cannot name them at
all. No PRD.

## Summary

An operator start cannot make anything a peer holds, so its undo never needs the session token it
does not have. A session delete that would be refused is refused before it touches the session's
agent or pointer.

## Scope

- [x] `start_session_for_os_user` refuses the four peer-held fields with `INVALID_ARGUMENT`, naming the field, before anything is created
- [x] `TODO(operator-start-undo)` removed (`start_task.rs:148-151`), and the `undo_a_timed_out_start` doc says an operator start has no peer-held part to undo
- [x] `delete_session_as` checks every part can go before the tmux kill and the pointer removal: a cross-host paired half or a recorded clone with no common room configured or joined is refused with the session intact
- [ ] Wrap: delete `docs/dev/todo/2026-11-07-operator-start-undo-of-peer-held-parts.md`, recording "closed as unreachable" in the change-history entry

## Technical Changes

### State A

Read at #857's base, #836's tip `ddef5cc6b`.

- `start_session_for_os_user` refuses these, in this order:
  - a session token (:32-36);
  - a peer daemon (:37-41);
  - a requested role;
  - a conflicting tmux run.

  It then applies the harness profile and runs `start_here`. Nothing refuses
  `codebase_daemon_instance_id`, `sandboxed_codebase`, `specialized_agents` or `clone_placed_agents`.
- `start_here` (`svc_start_session_core.rs:151-182`) runs every start under `StartDeadline` and
  undoes a late answer with `undo_a_timed_out_start`, keyed by OS user. The session token it passes
  is empty for an operator start.
- `delete_session_as` runs its steps in this order:
  1. kill tmux (:109-114);
  2. remove the session pointer (:117-124);
  3. delete the paired codebase half by token (:129-131);
  4. tear down every agent clone by token (:136-138);
  5. the local parts.

  A step 3 or 4 refusal therefore lands after the agent is already dead and its pointer gone.
- The project-home start already refuses placements in the same way, with `INVALID_ARGUMENT`
  (`project_home_start_spec.rs:24-50`).

### State B

- An operator start that names a peer-held part is refused, naming the field, before any
  directory, worktree or branch exists.
- `delete_session_as` runs a pre-check before step 1. It refuses with the session intact when a
  peer-held part could not be removed. Pass 2 set the conditions to what the teardowns themselves
  refuse on before reaching a peer:
  - a codebase half on another daemon, with no common room configured
    (`common_room_slot`, `tddy-host-service/src/peer_routing.rs:139`) or none joined
    (`svc_paired_codebase_teardown.rs:128-140`);
  - a recorded agent clone, under the same two conditions (`svc_provision_agent_clone.rs:184-193`).

  "No token" is not a condition: once the operator refusal lands, no entry reaches
  `delete_session_as` with an empty token and a peer-held part, so it would be untestable code.

## Testing Plan

- The operator refusal: unit tests on `refuse_peer_held_parts` in `tddy-agent-launch`. No public
  entry can carry the fields: `HostSessionStarter` builds its request from `OperatorSessionStart`,
  which has no such fields, and `DaemonSessionHost::launch_sessions` is `pub(crate)`. Reaching
  `start_session_for_os_user` from an integration test would need a new public accessor for tests.
- The delete reorder: an integration test in `tddy-session-lifecycle/tests/`, through the user
  `DeleteSession` of a split session on a daemon with a common room configured but not joined. The
  session's agent runs in a real tmux session on a private tmux server (`support::PrivateTmux`). No
  LiveKit.
- The guard: `an_operator_start_that_answers_inside_its_grace_is_undone_by_its_operator_key`
  (`session_start_deadline_acceptance.rs:311`), kept green.

## Acceptance Tests

Pass 2 filled in the files.

| Test | File | Kind |
|---|---|---|
| `an_operator_start_naming_a_codebase_host_is_refused_naming_the_field` | `packages/tddy-agent-launch/src/operator_start_unit_tests.rs` | 🔴 red |
| `an_operator_start_asking_for_a_sandboxed_codebase_is_refused_naming_the_field` | same | 🔴 red |
| `an_operator_start_seeding_specialized_agents_is_refused_naming_the_field` | same | 🔴 red |
| `an_operator_start_placing_agents_on_clones_is_refused_naming_the_field` | same | 🔴 red |
| `an_operator_start_naming_no_peer_held_part_is_admitted` | same | guard ✅ |
| `an_operator_start_that_answers_inside_its_grace_is_undone_by_its_operator_key` | `packages/tddy-session-lifecycle/tests/session_start_deadline_acceptance.rs:311` | guard ✅ (reused, not duplicated) |
| `a_refused_delete_leaves_the_session_and_its_agent_running` | `packages/tddy-session-lifecycle/tests/refused_delete_keeps_the_session_acceptance.rs` | 🔴 red |
| `a_session_with_a_clone_is_refused_while_no_common_room_is_configured` | `packages/tddy-session-agents/src/svc_provision_agent_clone/clone_host_precheck_tests.rs` | 🔴 red (unit; `FAILED_PRECONDITION` naming the clone and the daemon holding it) |
| `a_session_without_clones_is_not_refused` | same | guard ✅ |

## Responsibility

- What an operator-keyed start may ask for.
- The order of `delete_session_as`'s teardown: pre-check first, then the destructive steps.

## Boundaries

- Does **not** add an OS-user-keyed peer delete, or a peer delete protocol that authenticates the
  daemon. The deferred record's "what would close it" is dropped as unreachable.
- Does **not** change a token-carrying start or its undo (`undo_a_cancelled_start`).
- Does **not** change the start deadline (#831).
- Does **not** refuse `agent_clone` or `split_agent` on an operator start. Project home refuses them
  (`project_home_start_spec.rs:40-41`). Pass 2 checked: neither creates a part this daemon's
  delete must remove on a peer. Both make this session the *workspace* half a peer asked for; it
  records `agent_*`, not `codebase_*`, so `split_pairing` answers `None` for it
  (`svc_paired_codebase_teardown.rs:97-98`), and the clone store is keyed by the facilitating
  daemon's session, not this one. Not widened.
- Does **not** refuse a delete over an empty session token: see State B.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/bounded-waits` (#831, `#agent-tools` 23, implemented below) | `StartDeadline`, `start_here`, `undo_a_timed_out_start`, `delete_session_as` keyed by OS user | narrows the undo's doc, removes its TODO, reorders `delete_session_as` | change the deadline, the grace or `StartDeadline` |
| `feature/agent-tools/opencode-route-inline` (#857, `#agent-tools` 29) | nothing; it is the branch base only | — | — |
| `feature/agent-tools/acp-subagent-leftovers` (#835, `#agent-tools` 27, green below) | nothing this PR consumes; its `svc_provision_agent_clone.rs` edits are below, so the roster pre-check sits on its tip | — | — |

## Draft PR contract

The first push after this plan, every body `TODO(operator-start-refusal): implement`:
- `tddy-agent-launch/src/operator_start.rs`: `pub(crate) fn refuse_peer_held_parts(req: &StartSessionRequest) -> Result<(), Status>`, called by `start_session_for_os_user` after the peer-daemon refusal;
- `tddy-agent-launch/src/svc_signal_delete_session.rs`: `LaunchSessions::refuse_a_delete_a_peer_would_refuse(&self, sessions_base: &Path, session_id: &str) -> Result<(), Status>` (async), called first in `delete_session_as`;
- `tddy-session-split/src/svc_paired_codebase_teardown.rs`: `SplitSessions::refuse_an_unreachable_paired_codebase(&self, sessions_base: &Path, session_id: &str) -> Result<(), Status>` (async);
- `tddy-session-agents/src/svc_provision_agent_clone.rs`: `AgentRoster::refuse_an_unreachable_clone_host(&self, session_id: &str) -> Result<(), Status>` (async);
- the failing tests above.

## Green wave

**Wave:** 3 of 3. Its own edges would place it in wave 1 (#831 is implemented below, and no test
needs another open node's behaviour), but it sits above #857 (wave 3 of 3) in the line, and the
waves along the line never step back, so it is lifted to wave 3 (user, 2026-11-07).
**Greenable independently:** yes, once #836 and #857 are green below it.
**Concurrent with:** #857's green, as far as its code goes; they touch no common file.
**Blocks:** nothing.

## Decisions & Trade-offs

- **Close as unreachable (user, 2026-11-07).** The alternative was an OS-user-keyed peer delete
  plus a daemon-authenticated peer protocol, which is the record's "what would close it". It
  costs a protocol change on two crates to undo something no operator start creates.
- **Reorder the delete (user, 2026-11-07).** Today a peer refusal arrives after the tmux kill, so
  the session survives with its agent dead. With the pre-check, a refusal leaves everything running.

  *Limit:* a pre-check covers only what is knowable locally: a missing token, or a missing room.
  A peer that is reachable at check time but fails at delete time still fails after the kill.

  *Alternative:* run the peer deletes **first**, then tmux. A peer half gone with the local session
  surviving is retryable, because both peer deletes are idempotent on "no such session"
  (`svc_paired_codebase_teardown.rs:120,163`; `svc_provision_agent_clone.rs:159,216`). Not chosen; recorded for the reviewer.
- **The refused-delete test uses a reachable refusal.** After the operator refusal, the empty-token
  undo can no longer happen through any public entry. Pass 2 drives the reorder with the user
  `DeleteSession` of a split session while the common room is configured but not joined — the
  refusal `deleting_a_split_session_refuses_while_this_daemon_cannot_reach_the_common_room`
  (`remote_managed_worktree_acceptance.rs:705`) already pins, now with the session's agent running.
- **Refusal code: `INVALID_ARGUMENT` (user, 2026-11-07),** aligned with the neighbouring refusals on
  the same function (:32-41) and project home's placement refusal.
- **The operator refusal is tested as a unit (user, 2026-11-07).** No public entry can carry the
  four fields to `start_session_for_os_user` (see Testing Plan). Rejected: a `test_util` accessor
  for `DaemonSessionHost::launch_sessions`, which adds a public surface to a production crate for
  tests only.
- **The pre-checks live beside the teardowns (user, 2026-11-07)** (see Affected Packages), which
  costs two more crates. Rejected: a copy of the rule in `tddy-agent-launch` reading
  `split_pairing` and the clone store directly, which keeps one crate but lets the check and the
  teardown disagree.
- **The clone pre-check gets its own unit test (user, 2026-11-07)** in `tddy-session-agents`: the
  clone store on `DaemonSessionHost` is private, so no delete-level test can seed a clone. Its
  refusal names the clone and the daemon holding it, which the teardown's bare "no common room"
  refusal (`common_room_slot`) does not.
- **Complexity re-scored 2 → 4** in pass 2: three crates own symbols, and the teardown's order
  changes.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: internal fix)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (user, 2026-11-07)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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

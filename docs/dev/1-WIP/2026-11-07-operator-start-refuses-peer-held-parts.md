# Changeset: An operator start refuses peer-held parts, and a refused delete leaves the session intact

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 2 — a request refusal plus a reorder of one teardown, in `tddy-agent-launch`. The scoring rule reaches 3 or 4: the tests run through `DaemonSessionHost` in `tddy-session-lifecycle`, and a pre-check may need a read-only query on `tddy-session-split` and `tddy-session-agents`. It is kept at 2 as planned (see Decisions).

## Initial Discovery

[2026-11-07-operator-start-refuses-peer-held-parts-initial-discovery.md](./2026-11-07-operator-start-refuses-peer-held-parts-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-07-operator-start-undo-of-peer-held-parts.md](../todo/2026-11-07-operator-start-undo-of-peer-held-parts.md) | ✅ RESOLVED HERE, closed as **unreachable** (user, 2026-11-07): an operator start can no longer create a peer-held part, so its undo never needs a peer delete. Delete at wrap. |

No `Claimed by:` record exists for the touched files. `tddy-agent-launch/docs/code-issues/` holds
complexity records for `start_session_core` and other starts. None names `operator_start.rs`,
`start_task.rs` or `svc_signal_delete_session.rs`.

## Affected Packages

- **tddy-agent-launch**:
  - `operator_start.rs`: `start_session_for_os_user` (:26-57) refuses a request that would create a peer-held part. The refusal is `FAILED_PRECONDITION` naming the field, made before the harness profile, the placement or any step runs. The fields are:
    - `codebase_daemon_instance_id`;
    - `sandboxed_codebase`;
    - `specialized_agents`;
    - `clone_placed_agents`.
  - `start_task.rs`: `TODO(operator-start-undo)` (:148-151) goes. The `undo_a_timed_out_start` doc (:139-154) says an operator start has no peer-held part to undo.
  - `svc_signal_delete_session.rs`: `delete_session_as` (:90-179) checks that every part can be removed **before** it kills the tmux session (:109-114) or removes the session pointer (:117-124), so a refused delete leaves the session and its agent running.
- **tddy-session-split** and **tddy-session-agents**: only if pass 2 finds the pre-check needs one.
  - The candidates are a read-only "what would this delete need" query beside `SplitSessions::delete_paired_codebase_session` (`svc_paired_codebase_teardown.rs:78`) and `AgentRoster::tear_down_every_agent_clone` (`svc_provision_agent_clone.rs:253-268`).
  - Pass 2 decides whether the check fits in `tddy-agent-launch` alone.
- **tddy-session-lifecycle**: tests only. `DaemonSessionHost` builds the `LaunchSessions` both tests need.

## Related Feature Documentation

None. This is an internal fix with no new user-visible behaviour; an operator (Telegram) start never
sets these fields (`operator_session_starter.rs:55-80`). No PRD.

## Summary

An operator start cannot make anything a peer holds, so its undo never needs the session token it
does not have. A session delete that would be refused is refused before it touches the session's
agent or pointer.

## Scope

- [ ] `start_session_for_os_user` refuses the four peer-held fields with `FAILED_PRECONDITION`, naming the field, before anything is created
- [ ] `TODO(operator-start-undo)` removed (`start_task.rs:148-151`)
- [ ] `delete_session_as` checks every part can go before the tmux kill and the pointer removal
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
  peer-held part could not be removed. Pass 2 sets the exact set of conditions; the candidates are:
  - no token for a paired half or a clone;
  - no common room joined for a cross-host half (`svc_paired_codebase_teardown.rs:128-140`).

## Testing Plan

Integration tests in `tddy-session-lifecycle/tests/`, through `HostSessionStarter` and
`DeleteSession`, on the fixtures `session_start_deadline_acceptance.rs` already has: a project
whose worktree cut outlasts the deadline, `entries_under`, `worktrees_of` and `branch_exists`.

## Acceptance Tests

| Test | File | Kind |
|---|---|---|
| `an_operator_start_naming_a_codebase_host_is_refused_before_anything_is_created` (covers `codebase_daemon_instance_id` and `sandboxed_codebase`) | `packages/tddy-session-lifecycle/tests/` (new `operator_start_refusal_acceptance.rs`, or `session_start_deadline_acceptance.rs`: pass 2 decides) | 🔴 red |
| `an_operator_start_naming_seeded_or_clone_placed_agents_is_refused` | same | 🔴 red |
| `a_timed_out_operator_start_leaves_no_session_and_no_branch` | `packages/tddy-session-lifecycle/tests/session_start_deadline_acceptance.rs` | guard. Already pinned by `an_operator_start_that_answers_inside_its_grace_is_undone_by_its_operator_key` (:311); pass 2 renames or reuses it, and does not duplicate it |
| `a_refused_undo_leaves_the_session_and_its_agent_running` | `packages/tddy-session-lifecycle/tests/` (tmux-backed, like `claude_cli_tmux_session_acceptance.rs`) | 🔴 red |

## Responsibility

- What an operator-keyed start may ask for.
- The order of `delete_session_as`'s teardown: pre-check first, then the destructive steps.

## Boundaries

- Does **not** add an OS-user-keyed peer delete, or a peer delete protocol that authenticates the
  daemon. The deferred record's "what would close it" is dropped as unreachable.
- Does **not** change a token-carrying start or its undo (`undo_a_cancelled_start`).
- Does **not** change the start deadline (#831).
- Does **not** refuse `agent_clone` or `split_agent` on an operator start. Project home refuses them
  (`project_home_start_spec.rs:40-41`); pass 2 checks whether either creates a peer-held part and
  asks the user before widening.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/bounded-waits` (#831, `#agent-tools` 23, implemented below) | `StartDeadline`, `start_here`, `undo_a_timed_out_start`, `delete_session_as` keyed by OS user | narrows the undo's doc, removes its TODO, reorders `delete_session_as` | change the deadline, the grace or `StartDeadline` |
| `feature/agent-tools/opencode-route-inline` (#857, `#agent-tools` 29) | nothing; it is the branch base only | — | — |

## Draft PR contract

The first push after this plan:
- the owned surface: a named refusal helper for the operator start, e.g. `refuse_peer_held_parts(&StartSessionRequest)`, and the `delete_session_as` pre-check's signature, both with bodies `TODO(operator-start-refusal)`;
- the failing tests above.

## Green wave

**Wave:** 1 of 3, by its edges: #831 is implemented below, and no test needs another open node's behaviour.
**Greenable independently:** yes.
**Concurrent with:** every wave-1 node still open.
**Blocks:** nothing.

⚠ **The line steps back.** This wave-1 node sits above wave-3 nodes (#836, #857), so it cannot merge
before they do, although nothing it does needs them. The `pr-stack` rule forbids this. It was placed
here by the user's ordering (2026-11-07), and moving it is the user's call.

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
- **The refused-undo test must use a reachable refusal.** After the operator refusal, the empty-token
  undo can no longer happen through any public entry. Pass 2 drives the reorder with a reachable
  refusal instead, such as a user `DeleteSession` of a split session while no common room is joined.
- **Refusal code.** `FAILED_PRECONDITION`, as asked. The neighbouring field refusals on the same
  function (:32-41) and project home's placement refusal use `INVALID_ARGUMENT`; pass 2 asks
  whether to align.
- **Complexity kept at 2.** The scoring rule would read 3 (integration tests) or 4 (if the pre-check
  needs split or roster queries).

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: internal fix)
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

# Changeset: A child waits for an orchestrator on another host

**Created:** 2026-10-29
**Status:** 🚧 Contract published (pass 2): surface stubbed, tests failing; green pending
**Type:** Feature
**Complexity**: 4. It needs a new peer presence query over the existing unary peer forwarding, and two hosts in every acceptance test.
**Stack:** `#home-inbox 10/11` (PR #787), branch `feature/home-inbox/cross-host-wait`, base `feature/home-inbox/terminal-styles`
**PRD:** [PRD-2026-10-29-cross-host-answer-wait.md](../../ft/daemon/1-WIP/PRD-2026-10-29-cross-host-answer-wait.md)

## Initial Discovery

[2026-10-29-home-inbox-cross-host-wait-initial-discovery.md](./2026-10-29-home-inbox-cross-host-wait-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [`docs/dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md`](../todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md) | ✅ RESOLVED HERE |

## Affected Packages

- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): `orchestrator_is_present` stops refusing other hosts. `OrchestratorPresence` learns which host to ask.
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `ServedTerminalPresence` asks the peer, bounded, over the existing route classification and forwarding.
- [ ] [`tddy-service`](../../../packages/tddy-service/README.md): the peer presence query, either a new unary RPC or a field on an existing peer listing. Pass 2 picks the cheaper one.
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): two-host acceptance tests.

## Responsibility

- **Presence port.** `OrchestratorPresence::is_active` takes the orchestrator's host, e.g. `is_active(session_id, daemon_instance_id: Option<&str>)`. Pass 2 settles the exact shape.
- **Peer query.** `ServedTerminalPresence` answers a peer host by asking it, through the eligible-daemon roster and unary forwarding `PeerRoutedActivity` already uses. The peer is asked "do you serve `<session>`'s main terminal?" and answers with the same `get_terminal(.., MAIN_TERMINAL_ID)` rule a local check uses. The question is bounded by `PEER_PRESENCE_TIMEOUT` (a few seconds); a timeout or error means absent.
- **Wait gate.** `orchestrator_is_present` drops the `on_another_host` refusal and passes the host on.

## Boundaries

- The wait stays on the child's host. Nothing streams across hosts.
- No change to the answer tools, the authorisation, or the registry.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 6/11` permission-answer | `OrchestratorPresence`, `ServedTerminalPresence`, `orchestrator_is_present`, `AwaitQuestionAnswer` | widens presence to a peer | change the wait, the registry or the answer |
| `#home-inbox 8/11` question-answer | the question wait | covered by the same gate | change it |
| `#start-parity` #750 (merged) | `daemon_instance_id` routing and peer forwarding | the route to the peer | change routing |

## Draft PR contract

Published in pass 2. Every body is `// TODO(cross-host-wait): implement`, and the stub keeps today's
behaviour: another host counts as absent. The branch builds and lints clean on it.

| Surface | Where | Stub |
|---|---|---|
| `OrchestratorPresence::is_active(session_id, orchestrator_daemon_instance_id: Option<&str>)`. `None` is this daemon; the gate folds "empty or this host" into `None`. | `packages/tddy-session-activity/src/orchestrator_presence.rs` | `NoOrchestratorIsActive` ignores the host |
| `PEER_PRESENCE_TIMEOUT` (3 s) | same file | a constant |
| `rpc ActivityService.IsSessionTerminalServed(IsSessionTerminalServedRequest{session_id}) -> IsSessionTerminalServedResponse{served}`; TS regenerated | `packages/tddy-service/proto/activity.proto` | |
| Peer-side handler `is_session_terminal_served_impl`, wired in `service/rpc.rs` and delegated, not routed, by `PeerRoutedActivity` | `service/question_answers.rs` | answers `served: false` |
| `DaemonSessionHost::peer_serves_main_terminal(peer, session_id) -> bool`, called by `ServedTerminalPresence` for `Some(host)` | `packages/tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs` | `false` |
| `ServedTerminalPresence` now holds a `DaemonSessionHost` | same file | |

### The peer-RPC choice

A new unary RPC, because nothing existing tells "this daemon serves session X's main terminal":
`SessionEntry.is_active` (`ListSessions`) is pid liveness, the heuristic the todo rejects, and needs a
user token the hook does not hold; `ExecuteTool(ReadSessionTerminal)` needs a home-session caller
token; the terminal service has only a stream. It rides the existing roster and unary forward:
green classifies the host with `classify_addressed_daemon_route`, then
`common_room_slot(..).forward_to_peer_within(.., PEER_PRESENCE_TIMEOUT)`. `ActivityService` is
checked for inclusion only by `unbundle_service_split`, so nothing is registered there.

### Decisions

1. **The query is peer-only.** Green refuses `IsSessionTerminalServed` unless it arrives over
   `RequestTransport::LiveKit`, as `PERMISSION_DENIED` saying the RPC is peer-only. It carries no
   token, so the transport is the only gate.
2. **`PEER_PRESENCE_TIMEOUT` lives in `tddy-session-activity`**, beside the port, so tests import it;
   `tddy-session-lifecycle` applies it.
3. **Green drops the gate line.** `orchestrator_is_present` still returns `false` for another host
   (`TODO(cross-host-wait)` in `question_answers.rs`) so the stub keeps today's behaviour. Green
   removes it and passes `orchestrator_host` to the port, and the parent's
   `its_orchestrator_runs_on_another_host` case in `permission_answer_acceptance.rs` becomes
   host-aware (its fake presence must then answer for that host).
4. **Absent on any doubt.** An id off the roster, no common room, a peer error, a timeout or an
   undecodable answer all mean `false`, logged and never propagated.
5. **Parent test touch.** The two fake presence ports in `permission_answer_acceptance.rs` and
   `question_answer_acceptance.rs` take the new parameter and ignore it; nothing else changed.
6. **Test support.** `support/mod.rs` gained `never_answers`, `times_asked`, call recording and an
   `IsSessionTerminalServed` arm; all additive.

### Left for wrap

- `docs/ft/daemon/claude-cli-session.md` and the todo
  `docs/dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md` still describe the
  same-host-only rule; the todo is deleted when this node wraps.
- `packages/tddy-session-activity/docs/activity-service.md` lists the service's methods and its
  presence rule.

## Green wave

**Wave:** after `6/11` and `8/11`, which are both implemented.
**Greenable independently:** yes.
**Blocks:** nothing (`11/11` sweep sits above it).

## Testing Strategy

Acceptance: `packages/tddy-daemon-rpc/tests/cross_host_answer_wait_acceptance.rs` covers PRD
criteria 1-5 in the two-host world, with a remote orchestrator on a stand-in terminal and every wait
bounded. Unit: `packages/tddy-session-activity/tests/cross_host_presence_unit.rs`, over a recording
presence fake. All were verified once against a scratch implementation (6 of 6 acceptance cases
green, then reverted).

| Test | Checks | Fails today because |
|---|---|---|
| `a_forwarded_permission_answer_wakes_a_hook_whose_orchestrator_runs_on_another_host` (AC1) | the forwarded `AnswerSessionPermission` wakes the hook | the hook is told `NO_ORCHESTRATOR` at once |
| `a_forwarded_question_answer_wakes_a_hook_whose_orchestrator_runs_on_another_host` (AC2) | the same for `AnswerSessionQuestion` | same |
| `a_hook_is_told_nobody_is_there_at_once_when_the_other_host_does_not_serve_the_orchestrator` (AC3) | `NO_ORCHESTRATOR` in under 1.5 s, after exactly one query to host-b | no query is made |
| `a_hook_is_told_nobody_is_there_within_the_bound_when_the_other_host_never_answers` (AC4) | `NO_ORCHESTRATOR` within the bound plus 2 s, after one query to a peer that never answers | no query is made |
| `a_hook_is_told_nobody_is_there_at_once_when_asking_the_other_host_fails` (AC4) | the same for a peer that errors at once | no query is made |
| `an_orchestrator_on_the_childs_own_host_is_waited_for_and_no_peer_is_asked` (AC5) | same-host behaviour unchanged, zero queries | **passes: a guard** |
| `the_presence_query_is_refused_unless_it_arrives_from_a_peer` (decision 1) | Http, UnixSocket and InProcess get `PERMISSION_DENIED` naming peers | the stub answers every transport |
| `the_gate_asks_the_port_about_the_host_only_when_it_is_not_this_one` (4 cases) | none, empty and this host fold to `None`; another host is passed as `Some` | the another-host case: the gate never asks (the other three pass: guards) |
| `an_orchestrator_on_another_host_is_waited_for_when_the_port_says_it_is_active` (2 cases) | the port's answer for another host decides wait or `NO_ORCHESTRATOR` | the active case: the gate never asks (the inactive case passes: a guard) |
| `a_peer_is_told_whether_this_daemon_serves_the_session` (2 cases) | the handler answers from this daemon's port, naming no host | the stub never asks the port |

## TODO

- [x] Pass 2: contract and failing tests (acceptance and unit; user review approved by the orchestrator)
- [ ] TDD Green
- [ ] `/validate-changes`, `/pr-wrap`

## Validation Results

Not yet run.

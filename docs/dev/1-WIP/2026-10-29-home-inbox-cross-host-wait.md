# Changeset: A child waits for an orchestrator on another host

**Created:** 2026-10-29
**Status:** 🚧 Planned (pass 1: docs only)
**Type:** Feature
**Complexity**: 4. It needs a new peer presence query over the existing unary peer forwarding, and two hosts in every acceptance test.
**Stack:** `#home-inbox 10/11`, branch `feature/home-inbox/cross-host-wait`, base `feature/home-inbox/terminal-styles`
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

Pass 2 publishes the widened port, the peer query surface (proto when one is needed) and
`PEER_PRESENCE_TIMEOUT`. Bodies are `// TODO(cross-host-wait): implement`. The stub keeps today's
behaviour: another host is absent. Pass 2 also adds the failing two-host acceptance tests.

## Green wave

**Wave:** after `6/11` and `8/11`, which are both implemented.
**Greenable independently:** yes.
**Blocks:** nothing (`11/11` sweep sits above it).

## Testing Strategy

`packages/tddy-daemon-rpc/tests/cross_host_answer_wait_acceptance.rs` covers PRD criteria 1–5. It
uses two hosts in the existing two-host world, a remote orchestrator with a stand-in terminal, and
bounded waits.

## TODO

- [ ] Pass 2: contract and failing tests
- [ ] TDD Green
- [ ] `/validate-changes`, `/pr-wrap`

## Validation Results

Not yet run.

# Changeset: A split start streams the codebase host's progress, with no deadline

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (session-split, session-agents, agent-launch, daemon-kernel, session-lifecycle tests), ordering semantics across a forwarded stream, LiveKit peer seam
**Stack**: #start-progress 4/8. PR: #737. Base: `feature/start-progress/supervisor-handoff`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-split-stream-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md](../todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md) | ⚠ DURING → ✅ for the split part | This node gives split starts their phases (worktree relayed from the codebase host, agent on the agent host). The sandboxed, tool and orchestrator-child starts stay open: narrow the entry at wrap, do not delete it. |
| [2026-10-17-clone-passphrase-prompt-verification-gaps.md](../todo/2026-10-17-clone-passphrase-prompt-verification-gaps.md) | ⚠ DURING | Its first bullet is about the 330 s `start_forward_deadline`, which this node removes. Rewrite that bullet at wrap to the new bound (relay idle + keep-alives). The other two bullets stay. |
| [2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md) | ⚠ DURING | With the overall deadline gone, the relay's per-frame idle timeout is the only bound on a forwarded start. That bound already exists for streams; this node routes the unary forwards through a stream so they inherit it rather than hanging. No client-side deadline is added. |
| `packages/tddy-session-split/docs/code-issues/complexity-split-claude-cli-start-start-split-claude-cli-session.md` | ⚠ DURING | 146 lines. The forward-and-relay goes into a helper in a new module (`split_forward_stream.rs`); the function must not grow. |
| `packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` | ⚠ DURING | The agent phase brackets the call site, not the body. |
| `packages/tddy-session-split/docs/code-issues/complexity-svc-paired-codebase-teardown-delete-paired-codebase-session.md` | — | Not touched (`delete_paired_codebase_session`); only `tear_down_codebase_session`'s classification input changes. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ DURING | The `request_timeout` argument disappears from the workspace branch; no new branch. |
| `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md` | — | Not touched; streaming forwarding lives in the kernel. |

## Responsibility

Streamed forwarding of every start that crosses to a codebase host, and keep-alives for a start's
whole life. Owns:

- `tddy-daemon-kernel::peer_calls`: a forwarded start that **relays** the peer's `StartSessionEvent`s
  into a caller's sink and returns the peer's result — `forward_start_session_relaying(room, peer, req, sink)`.
  The unary `forward_start_session_within` and its deadline are deleted; every start forward goes
  through the stream, bounded by the relay idle timeout only.
- `tddy-session-split`: the split start (`start_split_claude_cli_session`) and `tddy-session-agents`'
  agent-clone provision (`svc_provision_agent_clone`) use it with the caller's real sink; the split
  start brackets the local agent spawn with `STEP_AGENT`.
- `SplitStartFailure` classified from the stream outcome: `PeerAnswered` for any status the peer sent
  (including `DEADLINE_EXCEEDED`), `PeerSilent` for the relay's idle expiry.
- `tddy-agent-launch`: a streamed start emits keep-alives every `START_KEEPALIVE_INTERVAL` from its
  first event to its last (not only around provisioning); the unary forward
  (`forward_start_session` in `start_request_checks.rs`) uses the relaying forward with a discarding sink.
- Removing `split_forward_deadline`, `start_forward_deadline`, and the `request_timeout` around the
  worktree cut in `start_workspace_session` / `start_agent_clone_session` on the session path.

## Boundaries

- Does not emit clone progress (node 5) or a clone step; relays whatever events the peer sends.
- Does not cancel anything (node 8); a caller that drops the stream leaves the peer running, as today.
- Does not change `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30 s) or `peer_forward_timeout_secs` (still the
  unary deadline for non-start forwards such as `DeleteSession`).
- Does not change the whole-session forward (`daemon_instance_id` = peer), already streamed.

## Dependencies

None in behaviour. The branch sits on `feature/start-progress/supervisor-handoff` (3/8) only because
the stack is a line.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/supervisor-handoff` (3/8) | injected `SpawnBackendChoice` in `svc_ensure_project_available_for_start.rs` | nothing | touch the backend choice |
| 1/8, 2/8 | spawn diagnostics, worker multiplexing | nothing | touch `tddy-spawn` |

## Draft PR contract

```rust
// tddy-daemon-kernel/src/peer_calls.rs
/// Forward StreamStartSession to `peer`, sending every non-terminal event to `sink` as it arrives and
/// returning the peer's result. Ends with the peer's status, or with the relay's idle status when the
/// peer is silent for PEER_FORWARD_STREAM_IDLE_TIMEOUT. No overall deadline.
pub async fn forward_start_session_relaying(room: &CommonRoom, peer: &str, req: &StartSessionRequest,
    relay: &(dyn Fn(StartSessionEvent) + Send + Sync)) -> Result<StartSessionResponse, ForwardedStartFailure>;
pub enum ForwardedStartFailure { PeerAnswered(Status), PeerSilent(Status) }
// deleted: forward_start_session_within

// tddy-session-split/src/split_start.rs
pub(crate) enum SplitStartFailure { PeerAnswered, PeerSilent }   // ForwardDeadline renamed; from ForwardedStartFailure

// tddy-session-agents/src/agent_roster.rs — split_forward_deadline deleted
// tddy-agent-launch/src/start_request_checks.rs — start_forward_deadline deleted
```

`AttachmentProgressSink::relay(StartSessionEvent)` (in `tddy-session-files`) is what callers pass as `relay` — a closure rather than a trait, so the kernel does not depend on `tddy-session-files`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — tests script the peer with an in-memory `PeerForwarder`
(`split_room_hosting_port.rs`'s `ACodebaseHost`, kernel `common_room_forwarding.rs`'s `AScriptedPeer`)
and paused clocks; no other node's behaviour.
**Concurrent with:** #734 (1/8), #735 (2/8), #736 (3/8)
**Blocks:** start-cancel (8/8) — cancel crosses the split hop on this stream

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — start forwarding
- **tddy-session-split**: [README.md](../../../packages/tddy-session-split/README.md) — split start, failure classification, workspace session
- **tddy-session-agents**: [README.md](../../../packages/tddy-session-agents/README.md) — agent-clone provision, deadline removed
- **tddy-session-files**: `AttachmentProgressSink` as `StartEventSink`
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — whole-life keep-alive, unary forward, deadline removed
- **tddy-session-lifecycle**: tests only (rewritten deadline tests)

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-split-stream.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-split-stream.md)
- [remote-managed-worktree.md](../../ft/daemon/remote-managed-worktree.md) § Failure is atomic — rewritten at wrap
- [clone-ssh-passphrase.md](../../ft/daemon/clone-ssh-passphrase.md) — the forwarded-start AC rewritten at wrap

## Summary

Every start forwarded to a codebase host is a relayed stream bounded only by peer silence, and every
streamed start keeps itself alive for its whole life.

## Background

The split forward is unary with a 330 s budget (`spawn_worker_request_timeout + peer_forward_timeout`);
the peer runs it with a discarding sink, so the operator sees nothing until it ends, and the peer's own
`DEADLINE_EXCEEDED` (its 30 s clone timeout on `mantas-udoo-ros`) was misreported as the forward's
deadline. Keep-alives only cover provisioning, so a long worktree cut on a streamed hop would trip the
30 s relay idle timeout.

## Scope

- [ ] `forward_start_session_relaying` + `StartEventSink`; `forward_start_session_within` deleted
- [ ] Split start and agent-clone provision relay the peer's events into the caller's sink
- [ ] `STEP_AGENT` around the split agent spawn
- [ ] `SplitStartFailure::{PeerAnswered, PeerSilent}` from the stream outcome
- [ ] Whole-life keep-alive for streamed starts
- [ ] Unary start forward rides the relaying forward
- [ ] `split_forward_deadline`, `start_forward_deadline`, worktree `request_timeout` on the session path removed
- [ ] Docs: `remote-managed-worktree.md` § Failure is atomic, `clone-ssh-passphrase.md` AC, kernel docs

## Technical Changes

### State A
`split_claude_cli_start.rs:95` and `svc_provision_agent_clone.rs:95` call
`forward_start_session_within(.., split_forward_deadline)`; the peer's unary handler uses
`AttachmentProgressSink::discarding()`. `from_forward_error` maps any `DeadlineExceeded` to
`ForwardDeadline`. `with_keepalive` wraps only `provision_project_for_start`.
`start_workspace_session(.., request_timeout)` wraps the worktree cut in `tokio::time::timeout`.

### State B
Both call `forward_start_session_relaying(.., sink)`; the peer serves `StreamStartSession` with a
streaming sink; its phase, keep-alive and attachment events reach the caller in order; the result
completes the forward. A peer status is `PeerAnswered`; relay idle expiry is `PeerSilent` (the only
case that keeps the orphan warning). The streamed handler emits a keep-alive every 5 s until its
terminal event. No timer bounds a start on the session path.

## Testing Plan

In-memory: kernel relay tests (paused clock) for event order, result, peer status vs silence; split
tests with `ACodebaseHost` scripting a streamed workspace start; agent-launch keep-alive test across a
slow worktree step. The cross-host LiveKit suite (`tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs`)
gains one case run by CI (needs the testkit container).

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_forwarded_start_relays_the_peers_events_in_order_then_returns_its_result` | `packages/tddy-daemon-kernel/tests/common_room_forwarding.rs` | relay + result |
| `a_peer_status_is_its_answer_and_peer_silence_is_silence` | same | `PeerAnswered` vs `PeerSilent` |
| `a_split_start_relays_the_codebase_hosts_worktree_step_and_keep_alives` | `packages/tddy-session-split/tests/split_room_hosting_port.rs` | split path, real sink |
| `a_split_start_whose_codebase_host_answers_deadline_exceeded_reports_that_host` | same | classification + teardown `PeerAnswered` |
| `a_split_start_outlives_the_old_budget_while_the_codebase_host_keeps_it_alive` | `packages/tddy-session-lifecycle/tests/remote_managed_worktree_acceptance.rs` (replaces the two deadline tests) | no overall deadline |
| `a_streamed_start_sends_keep_alives_through_a_slow_worktree_step` | `packages/tddy-agent-launch/tests/start_keepalive.rs` | whole-life keep-alive |
| `a_split_start_streams_the_codebase_hosts_progress_across_livekit` | `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` | end to end (CI) |

`packages/tddy-agent-launch/tests/start_forward_deadline.rs` is deleted with the deadline it pins.

⚠ **Written at the contract commit:** the two kernel relay tests (plus `a_peer_that_goes_silent_is_reported_as_silent`)
and the two split tests. **Deferred to `/green`, not yet written:** `a_split_start_outlives_the_old_budget_while_the_codebase_host_keeps_it_alive`
(it replaces the two `remote_managed_worktree_acceptance.rs` deadline tests, which green rewrites together with
removing the deadline), `a_streamed_start_sends_keep_alives_through_a_slow_worktree_step` (needs a seam for a slow
step that `/green` introduces with the whole-life keep-alive), and the LiveKit cross-host case (CI-only).

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Relay idle timeout is the bound**, guaranteed not to fire on a live start by whole-life keep-alives
  (developer's choice: no overall deadline, keep the idle timeout).
- **Unary forwards ride the stream** rather than keeping a deadline or hanging forever.
- **`PeerSilent` keeps the orphan warning**: after silence the peer may still create the session; node 8's
  cancel is what closes that.

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

## TODO

- [x] Record initial discovery (`2026-10-24-start-progress-split-stream-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer for the whole of pass 2, 2026-10-24)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests (`./test -p tddy-daemon-kernel -p tddy-session-split -p tddy-session-agents -p tddy-agent-launch -p tddy-session-lifecycle`); full-workspace result (incl. the LiveKit cross-host case) from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

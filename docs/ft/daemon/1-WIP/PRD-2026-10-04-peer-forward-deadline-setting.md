# Peer-forward deadline setting - PRD

**Date**: 2026-10-04
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [LiveKit peer discovery](../livekit-peer-discovery.md) - a daemon forwards an RPC to a peer over the common room; the time it waits for the peer is a fixed 30 s today and becomes an operator setting.
- **Related Feature 1**: [Remote managed worktree](../remote-managed-worktree.md) - its split-start deadline is `spawn_worker_request_timeout + PEER_FORWARD_TIMEOUT` and must follow the setting rather than the constant.
- **Related Feature 2**: [Session agent roster](../session-agent-roster.md) - its forwarded streams keep a fixed idle timeout; this PRD states that the idle timeout is deliberately not part of the setting.

## Summary

Add one daemon setting, `peer_forward_timeout_secs` (default 30), that bounds how long a daemon waits for a peer in the common room to answer a forwarded unary RPC or to open a forwarded server stream. The value is carried with the common-room handle every forwarding call already receives, so no call site reads a global.

## Background

`PEER_FORWARD_TIMEOUT` is a 30 s constant in `tddy-daemon-kernel::peer_forwarding`. A peer that is in the room but silent is exactly the case the deadline exists for, so the deadline cannot be removed and cannot be tested without waiting it out: the one test that pins it (`a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline`) takes about 31 s of a CI leg that is otherwise bounded by the sum of its tests. Operators have also had no way to shorten the wait on a fleet with fast links or lengthen it on a slow one. A production setting with a production default serves both without a branch that only tests take.

## Proposed Changes

### What's Changing

- `DaemonConfig` gains `peer_forward_timeout_secs` (u64, default 30), with an accessor that clamps to at least one second, following `spawn_worker_request_timeout_secs`. The name uses `_secs` to match the other duration keys on `DaemonConfig`.
- The forwarding entry points take the deadline from the common-room handle they already receive. The handle becomes a small shared type that holds the room slot and the deadline, built once from `DaemonConfig`.
- The unary deadline and the stream-**open** deadline follow the setting. The error text still names the seconds waited, and a silent peer still produces `DeadlineExceeded`.
- `split_forward_deadline` (remote managed worktree) uses the configured value.
- `daemon.yaml.production` documents the key as a commented example.
- The deadline test sets 2 s and still asserts `DeadlineExceeded`, so a missing deadline cannot pass as an up-front refusal.
- `rpc_scenarios` (one 30 s test running ten scenarios in sequence) becomes one test per scenario, so a failure names the scenario and the scenarios can run in parallel once the LiveKit tests are no longer serialised.

### What's Staying the Same

- The default is 30 s: a daemon with no new key behaves exactly as today.
- `PEER_FORWARD_STREAM_IDLE_TIMEOUT` stays a fixed constant. It is bounded below by the roster's notion of a pass long enough to count as service and is pinned by a test and a compile-time assert; it is not an operator knob.
- A peer that has left the common room is still refused immediately (`refuse_departed_daemon`), not waited out.
- No production code branches on being under test.

## Impact Analysis

### Technical Impact

- About 24 call sites (17 unary, 7 stream) take the handle instead of the bare room slot, across `tddy-daemon-kernel`, `tddy-daemon-livekit`, `tddy-host-service`, `tddy-session-lifecycle`, `tddy-session-agents` and `tddy-daemon-rpc`. The edit is a type change at the seam, not 24 new arguments.
- Several touched files are already over the size guideline (`config.rs`, `livekit_peer_discovery.rs`); the change must not grow them beyond a field, an accessor and the signature edits, and the `oversized-file-config` record is updated.
- A process-wide value was rejected: the e2e tests run several in-process daemons, which would share it.

### User Impact

- No change unless an operator sets the key. A value of 0 is treated as 1 second, as with the spawn timeout.
- No migration; the key is optional.

## Implementation Plan

1. Add the config key, default, accessor and tests in `tddy-daemon-kernel`.
2. Introduce the handle type and move the forwarders onto it, deadline included; keep the idle timeout constant.
3. Switch the remaining call sites and `split_forward_deadline`.
4. Rewrite the deadline test to set 2 s; update its comments and the constant's documentation.
5. Split `rpc_scenarios` one test per scenario, each with its own room.
6. Update `daemon.yaml.production`, the feature docs and the changeset docs.

## Acceptance Criteria

- [ ] `peer_forward_timeout_secs` defaults to 30 and is read from the daemon yaml ([LiveKit peer discovery](../livekit-peer-discovery.md))
- [ ] A forwarded unary RPC and a forwarded stream open both give up after the configured time with `DeadlineExceeded` naming that time
- [ ] The remote managed worktree split-start deadline follows the setting ([Remote managed worktree](../remote-managed-worktree.md))
- [ ] The stream idle timeout is unchanged and its guard test still passes ([Session agent roster](../session-agent-roster.md))
- [ ] The silent-peer deadline test runs in a few seconds with the setting at 2 s
- [ ] `rpc_scenarios` is split into one test per scenario with no scenario lost
- [ ] Tests passing for `tddy-daemon-kernel`, `tddy-daemon-livekit`, `tddy-host-service`, `tddy-session-agents`, `tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-livekit`, `tddy-daemon`

## References

### Affected Features (Complete List)

- [LiveKit peer discovery](../livekit-peer-discovery.md) - the setting and its default
- [Remote managed worktree](../remote-managed-worktree.md) - `PEER_FORWARD_TIMEOUT` is referenced at its split-start deadline
- [Session agent roster](../session-agent-roster.md) - idle timeout deliberately excluded

### Related Documentation

- Source note: [2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md](../../../dev/todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md) § 2
- Code issues in the path: `tddy-daemon-kernel/docs/code-issues/oversized-file-config.md`, `heavy-dependency-livekit-peer-forwarding.md`; `tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md`

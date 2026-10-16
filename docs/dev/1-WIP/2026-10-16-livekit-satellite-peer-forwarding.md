# Changeset: Peer forwarding through an extension port

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #624
**Type**: Refactor (architecture)
**Stack**: #satellite 2/13. Base: `satellite-1-extension-ports` (#623).

## Initial Discovery

[2026-10-16-livekit-satellite-peer-forwarding-initial-discovery.md](./2026-10-16-livekit-satellite-peer-forwarding-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md` | ✅ RESOLVED HERE | The kernel's only SDK consumer moves behind the port. Wrap records the final measurement and deletes the record. |
| [2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md) | — unrelated | `CommonRoom` keeps the deadline it already applies to the daemon→daemon hop; the client-side gap is unchanged. |
| `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` | ⚠ DURING | `config.rs` is not edited; `peer_forward_timeout()` is only read. |

## Responsibility

The transport behind daemon-to-daemon forwarding. Owns `PeerForwarder` and `NoPeerForwarding` in
`tddy-daemon-extensions`, `CommonRoom`'s port-based construction, and `LiveKitPeerForwarder` in the
plugin. Strikes `tddy-daemon-kernel` and `tddy-host-service` from `KNOWN_OFFENDERS`.

## Boundaries

- Does not move any *consumer* of forwarding: `tddy-daemon-rpc`, `tddy-session-files` and
  `tddy-daemon-auth` are node 10; `tddy-session-agents`, `tddy-session-split`,
  `tddy-session-lifecycle` and `tddy-agent-launch` are node 11. Their call sites change only as far
  as the new constructor and `is_connected()` force them to.
- Does not touch peer *discovery* (the advertisement loop, eligible daemons, signing keys).
- Does not add a client-side deadline to `tddy_livekit::RpcClient`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | The `tddy-daemon-extensions` crate (LiveKit-free, `tddy-rpc` + `async-trait` only) and the ratchet's `KNOWN_OFFENDERS` | Adds `peer_forwarder.rs` to that crate; strikes two entries from the list | Change `SessionReach`, the status board, or the ratchet's allowed sets |

**Sequencing (found writing the tests):** the ratchet assertions in
`livekit_is_a_satellite.rs` run on #623's `WorkspaceGraph`, still `todo!()` on #623 — they cannot
pass here until #623 is green. Every other test in this node fails on this node's own surface.

## Draft PR contract

The first push of this PR publishes:

```rust
// tddy-daemon-extensions/src/peer_forwarder.rs
#[async_trait::async_trait]
pub trait PeerForwarder: Send + Sync {
    /// Whether a peer can be reached at all right now (the transport is up).
    async fn is_connected(&self) -> bool;
    /// One unary call to `daemon-{peer_id}`. No deadline: `CommonRoom` applies it.
    async fn call_unary(&self, peer_id: &str, service: &str, method: &str, body: Vec<u8>)
        -> Result<Vec<u8>, Status>;
    /// A server stream from `daemon-{peer_id}`; the receiver stays fed until the peer ends it.
    async fn call_server_stream(&self, peer_id: &str, service: &str, method: &str, body: Vec<u8>)
        -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status>;
}
/// No transport wired: nobody is connected, every call is a precondition failure.
pub struct NoPeerForwarding;

// tddy-daemon-kernel/src/peer_forwarding.rs
impl CommonRoom {
    pub fn new(forwarder: Arc<dyn PeerForwarder>, config: &DaemonConfig) -> Self;
    pub async fn is_connected(&self) -> bool;
    // forward_timeout / forward_to_peer / forward_to_peer_within /
    // forward_server_stream_to_peer: signatures unchanged
}

// tddy-daemon-livekit/src/livekit_peer_forwarder.rs
pub struct LiveKitPeerForwarder { /* the common-room Room slot */ }
impl LiveKitPeerForwarder {
    pub fn new(slot: Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>) -> Arc<Self>;
}
impl PeerForwarder for LiveKitPeerForwarder { .. }
```

The failing tests listed under Acceptance Tests and the unit tests are published with it. The
bodies are `todo!()` until green.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes. Its tests drive `CommonRoom` over an in-memory forwarder and the
plugin over the existing testkit suites; nothing waits on another wave-2 node.
**Concurrent with:** nodes 3–9
**Blocks:** node 10 (forwarding-consumers) and node 11 (agent-clones), which route through
`PeerForwarder`

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-daemon-kernel`, `tddy-daemon-livekit`, `tddy-host-service`,
`tddy-daemon` (runtime wiring), `tddy-session-agents` and `tddy-session-split` (one `is_connected`
call each).

## Summary

Forwarding to a peer daemon stops requiring the kernel to know LiveKit. The forwarding policy stays
in the kernel and the transport moves to the plugin.

## Scope

- [ ] `PeerForwarder` + `NoPeerForwarding` in `tddy-daemon-extensions`
- [ ] `CommonRoom::new` / `is_connected`; SDK import removed from the kernel
- [ ] `LiveKitPeerForwarder` in the plugin; `peer_routing.rs`, `livekit_peer_discovery.rs`, runtime and host-service repointed
- [ ] `is_connected()` at `svc_provision_agent_clone.rs:195` and `svc_paired_codebase_teardown.rs:136`
- [ ] Manifests: `tddy-daemon-kernel`, `tddy-host-service` drop `livekit` / `tddy-livekit`
- [ ] Ratchet: strike `tddy-daemon-kernel`, `tddy-host-service`

## Technical Changes

### State A
- `packages/tddy-daemon-kernel/src/peer_forwarding.rs:18` `use livekit::Room;`. `CommonRoom { slot:
  Arc<RwLock<Option<Arc<Room>>>>, forward_timeout }` (`:115`). `peer_client` builds a
  `tddy_livekit::LiveKitRpcClientFactory::for_room(room).client(daemon_rpc_identity(peer))`.
  `forward_to_peer_within` wraps `call_unary` in `tokio::time::timeout`, and
  `forward_server_stream_to_peer` relays frames with `PEER_FORWARD_STREAM_IDLE_TIMEOUT`, holding
  the client alive inside the relay task.
- Constructors: `tddy-daemon-livekit/src/peer_routing.rs:40`,
  `livekit_peer_discovery.rs:680`, `tddy-daemon/src/runtime.rs:1571`,
  `tddy-host-service/src/service.rs:148-149` (`with_common_room(slot)`), plus tests
  `forward_to_peer_shared_registry.rs:91` and `forwarded_rpc_is_stamped_by_the_receiver.rs:118`.
- Slot readers: `tddy-session-agents/src/svc_provision_agent_clone.rs:195`,
  `tddy-session-split/src/svc_paired_codebase_teardown.rs:136` (`slot().read().await.is_none()`).
- Kernel manifest `:26` `tddy-livekit`, `:29` `livekit = "0.7"`. Host-service manifest `:29`
  `livekit = "0.7"` (used only to name `Room` in `with_common_room`).

### State B
`CommonRoom` holds `Arc<dyn PeerForwarder>`. `peer_client` disappears; `forward_to_peer_within`
calls `forwarder.call_unary` under the same timeout, and the stream relay reads
`forwarder.call_server_stream`'s receiver with the same idle timeout and terminal-error rule. The
plugin's `LiveKitPeerForwarder` reproduces today's client construction, its "common room is not
connected" precondition, and the keep-alive of the client for a stream's life.

### Delta
| Package | Change |
|---|---|
| `tddy-daemon-extensions` | `src/peer_forwarder.rs`, `lib.rs` export; `tokio` (`sync`) dependency |
| `tddy-daemon-kernel` | `peer_forwarding.rs`; `Cargo.toml` (−`livekit`, −`tddy-livekit`, +`tddy-daemon-extensions`); `tests/common_room_forwarding.rs` (new) |
| `tddy-daemon-livekit` | `src/livekit_peer_forwarder.rs` (new), `peer_routing.rs`, `livekit_peer_discovery.rs`, `lib.rs`; two tests' constructors |
| `tddy-host-service` | `service.rs` `with_common_room(CommonRoom)`; `Cargo.toml` (−`livekit`); `host_tooling_handler_unit_tests.rs` |
| `tddy-daemon` | `runtime.rs` wiring |
| `tddy-session-agents`, `tddy-session-split` | one `is_connected()` call each |
| `tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | strike two offenders |

## Implementation Milestones
- [ ] M1 Port and null object with unit tests green
- [ ] M2 `CommonRoom` over the port; kernel tests green with an in-memory forwarder
- [ ] M3 Plugin forwarder; existing `tddy-daemon-livekit` forwarding suites green
- [ ] M4 Manifests trimmed; ratchet green

## Testing Plan

The policy (deadline, idle timeout, errors) is tested in the kernel against an in-memory
`PeerForwarder` fake, with no LiveKit, which is the point of the port. The transport keeps its
existing testkit-backed suites in `tddy-daemon-livekit` (`forward_to_peer_shared_registry.rs`,
`forwarded_rpc_is_stamped_by_the_receiver.rs`, `livekit_peer_daemons_acceptance.rs`); they change
only in how they build a `CommonRoom`. The ratchet asserts the dependency outcome.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_forwarded_call_returns_what_the_peer_answered` | `packages/tddy-daemon-kernel/tests/common_room_forwarding.rs` | `forward_to_peer` relays the forwarder's reply bytes |
| `a_peer_that_does_not_answer_within_the_deadline_is_reported_as_timed_out` | same | `forward_to_peer_within` → `DeadlineExceeded` naming service, method, peer and seconds |
| `forwarding_without_a_connected_common_room_is_a_failed_precondition` | same | `CommonRoom::new(Arc::new(NoPeerForwarding), ..)` refuses; `is_connected()` is false |
| `a_relayed_stream_delivers_each_frame_then_ends_when_the_peer_ends` | same | Frames decoded in order; end-of-stream closes the relay |
| `a_stream_that_goes_quiet_ends_with_a_deadline_error` | same | Idle past `PEER_FORWARD_STREAM_IDLE_TIMEOUT` → one terminal `DeadlineExceeded` frame (uses `tokio::time::pause`) |
| `a_common_room_carries_the_deadline_the_config_names` | `packages/tddy-daemon-kernel/src/peer_forwarding.rs` (existing, rebuilt on `new`) | `forward_timeout()` from config |
| `the_kernel_and_host_service_name_no_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | `KNOWN_OFFENDERS` without the two crates; `every_known_offender_still_names_a_livekit_crate` and `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` pass |

Unit (`tddy-daemon-extensions/src/peer_forwarder.rs`):
`with_nothing_wired_no_peer_is_connected`, `with_nothing_wired_a_forward_is_a_failed_precondition`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Port the transport, keep the policy.** Deadlines and the idle rule are behaviour every
  transport must share; putting them in the port would let a second transport silently drop them.
- **`is_connected` instead of exposing the slot.** The two consumers only asked "is the room up";
  exposing a slot type would put the SDK back in their signatures.
- **The extensions crate gains `tokio` (`sync`)** for the stream receiver type. `tokio` reaches no
  LiveKit crate, so the ratchet's transitive assertion still holds.
- **`tddy-vm` is untouched.** Its `CommonRoom` matches are its own `LiveKitCommonRoom` config
  struct (`tddy_host.rs:74`), not the kernel type.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-peer-forwarding-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete

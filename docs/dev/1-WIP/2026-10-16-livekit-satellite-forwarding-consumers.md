# Changeset: Forwarding consumers off the LiveKit plugin

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #632
**Type**: Refactor (architecture)
**Stack**: #satellite 10/13. Base: `feature/satellite/status-ui` (#631).

## Initial Discovery

[2026-10-16-livekit-satellite-forwarding-consumers-initial-discovery.md](./2026-10-16-livekit-satellite-forwarding-consumers-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md` | ⚠ DURING | Import rewrites only. |
| `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` | ⚠ DURING | `auth.rs` is not edited here (#626 moved its minting). |
| `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md` | ✅ partly, narrowed | The 13 typed helpers (about 300 lines) leave the file. Narrow the record at wrap. |

## Responsibility

The typed peer calls and `PeerRouting` as kernel code over `CommonRoom`; daemon-rpc's and
session-files' switch to them; the OAuth loopback tunnel's move into the plugin. Strikes
`tddy-daemon-rpc`, `tddy-session-files` and `tddy-daemon-auth`.

## Boundaries

- Does not change `CommonRoom` or `PeerForwarder`, which belong to #624.
- Leaves the session-forwarding callers in session-agents, session-split and agent-launch on the
  plugin's deprecated re-exports; node 11 moves them.
- Does not touch peer *discovery*.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #624 peer-forwarding | `CommonRoom::new(Arc<dyn PeerForwarder>, config)`, `is_connected`, unchanged `forward_to_peer*` | The moved helpers and `PeerRouting` call them | Change their signatures or the port |
| #626 room-tokens | daemon-auth minting through `RoomTokenMinter` (no `TokenGenerator` left in `auth.rs` or `token_provider.rs`) | Lets daemon-auth be struck once the tunnel leaves | Touch minting |
| #623, #625, #627–#631 | Ratchet list (#623); nothing else consumed | Strikes three entries | — |

**New edge found while planning:** `n4 → n10`. #626's `## Green wave` lists only node 11 as blocked;
its wave-2 commit updates that (a child does not edit its parent's docs).

**As published (wave 2):** kernel `peer_calls` (13 helpers, `todo!()`), host-service
`peer_routing::PeerRouting` (`new`, `classify_daemon_route`, `common_room_slot`; the rest of its
surface moves in green), and failing tests. **Sequencing:** `tests/peer_calls.rs` builds a real
`CommonRoom` over an in-memory forwarder — the integration is the point — so it currently stops at
#624's `CommonRoom::new` (`todo!()` there) and cannot pass before #624 is green. The OAuth tunnel's move
is a pure relocation and lands in green. The ratchet strikes wait on #623's `WorkspaceGraph`.

## Draft PR contract

```rust
// tddy-daemon-kernel/src/peer_calls.rs
pub async fn forward_add_project_to_host(room: &CommonRoom, peer: &str, req: &AddProjectToHostRequest)
    -> Result<AddProjectToHostResponse, Status>;
pub async fn forward_set_project_default_branch(..) -> ..;
pub async fn forward_set_project_accounts(..) -> ..;
pub async fn forward_stream_execute_tool(..) -> ..;
pub async fn forward_read_host_document(..) -> ..;
pub async fn forward_stream_read_host_document(..) -> ..;
pub async fn forward_upload_staged_attachment_chunk(..) / forward_list_staged_attachments(..) / forward_delete_staged_attachment(..);
pub async fn forward_start_session(..) / forward_start_session_within(..) / forward_stream_start_session(..) / forward_delete_session(..);
// (same parameter and return types as today's `*_via_livekit`, minus the suffix)

// tddy-host-service/src/peer_routing.rs  (beside `EligibleDaemonSource`, which it needs)
pub struct PeerRouting { .. }
impl PeerRouting {
    pub fn new(config: DaemonConfig, eligible: Arc<dyn EligibleDaemonSource>, common_room: Option<CommonRoom>) -> Self;
    // eligible_daemon_source / common_room_livekit_room: unchanged accessors
}

// tddy-daemon-livekit/src/oauth_loopback_tunnel.rs  (moved from tddy-daemon-auth)
pub fn spawn_oauth_loopback_tunnel(..);   // signature unchanged
```

The first push moves no code. It publishes the new signatures with `todo!()` bodies and the
failing tests. `PeerRouting` goes to `tddy-host-service`, not the kernel: it needs
`EligibleDaemonSource` (`tddy_host_service::multi_host`), and `tddy-host-service` already depends on
the kernel (`Cargo.toml:24`), so the kernel cannot take it without a cycle.

## Green wave

**Wave:** 3 of 5
**Greenable independently:** not yet. The helpers' tests need #624's `CommonRoom` over a port
(behaviour), and the daemon-auth strike needs #626's minting move. Greenable once #624 and #626 are
green.
**Concurrent with:** none
**Blocks:** node 11 (its session-forwarding callers move onto the kernel helpers)

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-kernel`, `tddy-host-service`, `tddy-daemon-livekit`, `tddy-daemon-rpc`,
`tddy-session-files`, `tddy-daemon-auth`, `tddy-daemon` (runtime), `tddy-session-lifecycle` (its re-export of
`oauth_loopback_tunnel` and `peer_routing`).

## Summary

Forwarding a typed RPC to a peer, and routing a request to its owning daemon, become kernel code. The
one LiveKit-specific consumer, the OAuth tunnel, moves into the plugin.

## Scope

- [ ] Kernel `peer_calls` (13 helpers) + plugin `#[deprecated]` re-exports
- [ ] `tddy_host_service::peer_routing` (`PeerRouting`), with a constructor taking `Option<CommonRoom>`
- [ ] daemon-rpc (`exec_tool/ports.rs`, `project/coordinate_handlers.rs`, `project.rs`, `catalog/ports.rs`) repointed
- [ ] session-files (`svc_materialize_staged_attachment.rs`, `session_attachment_materialization.rs`) repointed
- [ ] OAuth tunnel → plugin; runtime and lifecycle re-export repointed
- [ ] Ratchet: strike three entries

## Technical Changes

### State A
- Helpers: `tddy-daemon-livekit/src/livekit_peer_discovery.rs:1363-1660`, thirteen `pub async fn
  forward_*_via_livekit(room: &CommonRoom, ..)`, each encoding the request, calling
  `room.forward_to_peer` / `forward_server_stream_to_peer`, and decoding.
- `peer_routing.rs:10` `use livekit::prelude::Room`; `PeerRouting::new(.., Option<Arc<RwLock<Option<Arc<Room>>>>>)` (`:34-40`).
- daemon-rpc: `exec_tool/ports.rs:10,119,201-317`; `project/coordinate_handlers.rs:11,225-523`;
  `project.rs:13` (`CommonRoom`); `catalog/ports.rs:18` (identity, moved by #627).
- session-files: `svc_materialize_staged_attachment.rs:12,33,132,155,230`;
  `session_attachment_materialization.rs:12`; `context_files.rs:49` (a doc comment only).
- daemon-auth: `oauth_loopback_tunnel.rs:11` (`livekit::prelude::{ParticipantIdentity, Room,
  RoomEvent}`), `:21` (`RpcClient`); public `run_oauth_tunnel_supervisor_follow_room_slot` `:260`,
  `run_oauth_tunnel_supervisor` `:290`, `spawn_oauth_loopback_tunnel` `:333`; started from
  `tddy-daemon/src/runtime.rs:290,375` through the lifecycle re-export. `signing_key.rs:318` only
  mentions `AdvertisedSigningKey` in a doc comment.

### State B
Consumers import `tddy_daemon_kernel::peer_calls` and `tddy_host_service::peer_routing`, the tunnel runs from the plugin,
and the three manifests drop `tddy-daemon-livekit` / `livekit` / `tddy-livekit`.

### Delta
As listed in Scope.

## Implementation Milestones
- [ ] M1 Kernel helpers + host-service routing with in-memory-forwarder tests
- [ ] M2 Consumers repointed
- [ ] M3 Tunnel moved; manifests; ratchet green

## Testing Plan

The typed helpers are pure encode, forward and decode over `CommonRoom`. They are tested against an
in-memory `PeerForwarder` that answers by service and method. The tunnel keeps its existing tests,
which move with it. The ratchet asserts the strikes.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `adding_a_project_on_a_peer_sends_the_request_to_that_peer_and_decodes_its_answer` | `packages/tddy-daemon-kernel/tests/peer_calls.rs` (new) | Service and method names, the encoded request, the decoded response |
| `a_peer_answering_garbage_is_reported_as_an_internal_decode_error` | same | Decode failure produces `Internal` naming the response type |
| `reading_a_host_document_from_a_peer_relays_every_frame_decoded` | same | Stream helper over the in-memory forwarder |
| `a_request_for_this_daemon_is_routed_locally_and_one_for_a_peer_is_forwarded` | `packages/tddy-host-service/tests/peer_routing.rs` (new) | `PeerRouting` with an eligible-source fake |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Three entries struck; both ratchet tests pass |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Helpers to the kernel, not the extensions crate.** They need the protos (`tddy-service`) and
  `CommonRoom` (kernel). The extensions crate stays a crate of ports.
- **Deprecated re-exports for one node.** They keep node 11's callers compiling without touching
  them here. Node 11 deletes the re-exports.
- **The tunnel moves rather than gets a port.** It *is* a LiveKit feature (it follows room
  participants), so a port would only wrap one implementation.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-forwarding-consumers-initial-discovery.md`)
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

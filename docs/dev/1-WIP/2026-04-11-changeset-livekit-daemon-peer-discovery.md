# Changeset: LiveKit daemon peer discovery (common room)

**Status:** Product documentation wrapped into **[docs/ft/daemon/livekit-peer-discovery.md](../../ft/daemon/livekit-peer-discovery.md)**; this file remains the operator / CI supplement.

## Plan context (summary)

Daemons that share `livekit.common_room` and credentials advertise in the room via participant metadata; **ListEligibleDaemons** returns the local row plus discovered peers. **StartSession** with a peer `daemon_instance_id` forwards over the existing LiveKit **tddy-rpc** data channel to that peer’s **ConnectionService**.

## Trust model (operators)

- **Security perimeter:** same LiveKit project URL, API key/secret, and `common_room` name. Any participant who can join that room may appear as an eligible host and receive a forwarded **StartSession** (full protobuf, including `session_token`).
- **Not** cryptographic attestation of “real” `tddy-daemon` software — use private projects, restricted networking, and trusted hosts.
- Rust module docs: `packages/tddy-daemon/src/livekit_peer_discovery.rs` (top-level `//!` section).

## CI / test environment

- **`TDDY_PROJECTS_DIR`:** test-only override for `projects_path_for_user`; see `packages/tddy-daemon/src/user_sessions_path.rs`. Do not set globally across unrelated suites; `multi_host_acceptance` uses save/restore + `#[serial]`.
- **LiveKit:** Docker testkit or `LIVEKIT_TESTKIT_WS_URL`; acceptance tests use `#[serial]` where they share LiveKit.

## Affected packages

- `packages/tddy-daemon` — discovery, registry, `LiveKitDiscoveryHandles`, `ConnectionServiceImpl` wiring
- `packages/tddy-livekit` — `RpcClient::new_shared` (`Arc<Room>`)
- `packages/tddy-web` — Host dropdown ordering, Cypress

## Implementation milestones

- [x] Common-room join, metadata advertisement, registry sync (events + 500 ms tick)
- [x] `LiveKitEligibleDaemonSource`, merge/dedupe, `StartSession` classify + forward
- [x] `LiveKitDiscoveryHandles` for constructor grouping
- [x] Acceptance + unit tests; `local_instance_id_for_config` shared with connection service

## Validation

- `./dev cargo test -p tddy-daemon -- --test-threads=1`
- `./dev cargo test -p tddy-livekit -- --test-threads=1`
- Cypress: `ConnectionScreen.cy.tsx` component spec (host multi-daemon)

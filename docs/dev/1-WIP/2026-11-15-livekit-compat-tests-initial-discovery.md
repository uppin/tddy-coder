# Initial discovery — LiveKit compatibility tests move out of the crates that own the logic

**Date:** 2026-11-15
**Source TODO:** `docs/dev/todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md`
(extended by `docs/dev/todo/2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md`)

## The rule being implemented

1. Non-LiveKit packages test **logic only**, over cheap transports (in-memory, stdio, local IPC).
   No `tddy-livekit-testkit` dev-dependency, no container, no network.
2. LiveKit is tested **only for connectivity/contract**: one dedicated package proves the LiveKit
   adapter of each port behaves like the cheap adapters (same envelopes, metadata, streaming,
   rejoin, deadline semantics). No logic use-cases live there.
3. That package is a **separate e2e check on a GitHub PR**, so a logic change does not pay for a
   LiveKit container and a LiveKit failure is attributable to the adapter.

## Measured state on this branch (re-measured, not copied from the TODO)

Test files importing `tddy_livekit_testkit` / `LiveKitTestkit` (`grep` over `packages/*/tests/*.rs`):

| Package | Files | Kind |
|---|---:|---|
| `tddy-daemon` | 11 | logic (cross-host behaviour over a real room) |
| `tddy-daemon-livekit` | 9 | LiveKit adapter contract |
| `tddy-livekit` | 9 | LiveKit transport contract |
| `tddy-e2e` | 4 | LiveKit contract (behind the crate's `livekit` feature) |
| `tddy-coder-cli` | 2 | spawned-participant connectivity/contract |
| `tddy-session-lifecycle` | 2 | logic (session room / sync over a real room) |
| `tddy-daemon-rpc` | 1 | logic (session-room exec tool) |
| `tddy-worktree-service` | 1 | logic (remote git over LiveKit) |
| `tddy-livekit-testkit` | 3 | the testkit's own tests (stay; they use the library, not a dev-dependency on itself) |

False positives excluded: `tddy-semantic-index`, `tddy-tools-cli`, `tddy-tools`, `tddy-vm-testkit`
mention `LIVEKIT_TESTKIT_WS_URL` only in comments.

## Cheap-adapter seams that already exist

- `tddy-daemon-rpc/tests/support/mod.rs` — `InProcessPeers`: two daemons in one process over an
  in-memory `PeerForwarder` (the `#start-parity` world). This is the seed the TODO asks to promote.
- `tddy-daemon-kernel/tests/common_room_forwarding.rs` — scripted `PeerForwarder` peers.
- `tddy-session-split/tests/split_room_hosting_port.rs` — scripted peer for the split-room port.
- `tddy-rpc` `ClientEngine`/`ServerEngine` over `RpcClientTransport`; `tddy-stdio` is one adapter.

The daemon's cross-host suites hand-wire two services through `LiveKitParticipant::connect`
(e.g. `tddy-daemon/tests/multi_host_acceptance.rs`), and `runtime::build` hard-wires
`LiveKitPeerForwarder` when `livekit:` is configured. Converting those suites therefore needs a
transport seam at the participant/peer layer, not a search-and-replace — it is the second phase.

## Analyser note (targeting)

`analyze-code-issues` (CRAP) chooses *complexity* targets. This change's targets are chosen by a
measured import (`tddy-livekit-testkit` in a test file), stated in the table above, so no CRAP
record is created or claimed by it. Duplicate-test detection is not applicable: suites move
verbatim, none is copied.

## CI state

- `.config/rust-e2e.filterset` (102 lines) mixes LiveKit binaries with supervisor, index-daemon,
  stdio and restructure binaries. `scripts/nextest-e2e-archive.ts` derives the e2e archive from it.
- `rust-e2e-shard` ×6 + the aggregator check `Rust e2e tests` in `.github/workflows/ci.yml` run it,
  with one shared LiveKit server per shard (`scripts/livekit-ci-server.sh start`).

## Slices

- **This changeset:** create `tddy-livekit-compat-tests`; move the 24 contract suites from
  `tddy-livekit` (9), `tddy-daemon-livekit` (9), `tddy-e2e` (4) and `tddy-coder-cli` (2) into it with
  `move_test_binary_to_crate`; pin placement with a test; give the package its own filterset and its
  own PR check (`Rust LiveKit e2e tests`); remove the moved binaries from `rust-e2e.filterset`.
- **Left in the TODO, narrowed:** converting the logic suites in `tddy-daemon` (11),
  `tddy-session-lifecycle` (2), `tddy-daemon-rpc` (1) and `tddy-worktree-service` (1) onto the
  in-memory adapters, and the media port. Those suites stay where they are in this slice, and the
  placement test names them as the remaining set so the list can only shrink.

## The remaining 15, by production surface (measured 2026-11-15)

Every remaining suite touches at least one surface with no port today, so the conversion is three
behaviour-preserving port extractions (in the TODO, clause 1), not 15 test edits:

| Suite | Discovery | Session room | Participant pair | Raw `Room` | `runtime::build` |
|---|:-:|:-:|:-:|:-:|:-:|
| `tddy-daemon/common_room_key_trust_acceptance` | ✓ | | | ✓ | ✓ |
| `tddy-daemon/first_login_enrolment_acceptance` | | | ✓ | ✓ | ✓ |
| `tddy-daemon/multi_host_acceptance` | ✓ | | ✓ | | |
| `tddy-daemon/relay_e2e_acceptance` | ✓ | | | | |
| `tddy-daemon/remote_managed_worktree_cross_host_acceptance` | ✓ | ✓ | | | |
| `tddy-daemon/runtime_signing_identity_acceptance` | | | | ✓ | ✓ |
| `tddy-daemon/session_agent_remote_acceptance` | ✓ | ✓ | ✓ | | |
| `tddy-daemon/session_attach_cross_host_acceptance` | ✓ | | | | |
| `tddy-daemon/session_room_cross_host_acceptance` | ✓ | ✓ | | ✓ | |
| `tddy-daemon/split_session_resume_acceptance` | ✓ | ✓ | | | |
| `tddy-daemon/staging_forwarding_acceptance` | ✓ | | ✓ | | |
| `tddy-session-lifecycle/session_room_acceptance` | | ✓ | | ✓ | |
| `tddy-session-lifecycle/session_sync_livekit_acceptance` | | ✓ | ✓ | | |
| `tddy-daemon-rpc/session_room_exec_tool_acceptance` | | ✓ | | ✓ | ✓ |
| `tddy-worktree-service/remote_git_livekit_acceptance` | | | ✓ | | |

Counts: discovery 9, session-room 7, participant pair 6, raw room 6, full runtime 4. The keystone
is the runtime seam: four suites build `runtime::build` with a `livekit:` config, so the discovery
port has to be carried on `RuntimeOptions` before any of them can move.

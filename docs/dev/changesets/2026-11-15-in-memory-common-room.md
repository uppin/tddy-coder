# 2026-11-15 — An in-memory common room for the logic suites

**Type:** Refactor

Second node of the LiveKit test split (after
[2026-11-15-livekit-compat-tests.md](2026-11-15-livekit-compat-tests.md)). Logic suites that
proved daemon routing/forwarding behaviour could only run over a real LiveKit room, because
common-room discovery had no transport-neutral seam. This node builds that seam and converts the
first suites onto it:

- `CommonRoomPeerRegistry` in `tddy-daemon-livekit` learned `sync_from_participant_fields`, a
  transport-neutral snapshot entry reusing the classifier the LiveKit path already delegates
  to. The LiveKit path is unchanged.
- New test crate `tddy-room-testkit`: an in-memory `RoomHub` where participants join named
  rooms under an identity with metadata, registries attach and stay synced, services are
  served and called through `RpcClientTransport` and a `PeerForwarder` addressed the way the
  kernel's `CommonRoom` addresses peers, and topics carry broadcasts. Forwarded calls arrive
  stamped `RequestTransport::LiveKit` with the caller's identity, exactly as a real room
  stamps them, because services decide trust on that stamp. A **silenced** participant model
  (in the room, discovered, never answering) supports deadline tests. The hub is test support
  only, named from `[dev-dependencies]`; no production crate depends on it.
- Converted in `tddy-daemon`, assertions unchanged: `relay_e2e_acceptance` (4),
  `session_attach_cross_host_acceptance` (8), `multi_host_acceptance` (6),
  `staging_forwarding_acceptance` (2). They now run in well under a second with no Docker.

Deliberately not converted: `tddy-worktree-service/remote_git_livekit_acceptance` — the shipped
`tddy-remote-git-repo` shim itself joins a LiveKit room, so that suite tests a LiveKit product
feature, not logic over a transport. It stays in `REMAINING_LOGIC_SUITES` (now 11 lines: the
session-room suites and the full-runtime suites are the following nodes).

Per-package detail is in each package's own entry under
`packages/<pkg>/docs/changesets/2026-11-15-in-memory-common-room.md`.

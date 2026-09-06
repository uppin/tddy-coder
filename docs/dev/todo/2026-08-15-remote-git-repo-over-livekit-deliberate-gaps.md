# 2026-08-15 — Remote git repo over LiveKit — deliberate gaps

**Category:** Future enhancement
**Source:** remote-git-repo-over-livekit changeset, 2026-08-15

- **No CLI login flow.** `tddy-remote-git-repo` takes a refresh token (`--refresh-token` /
  `TDDY_REFRESH_TOKEN`) and exchanges it via `auth.AuthService/RefreshSession`, but the only way to
  obtain one is to read `localStorage.tddy_refresh_token` out of the web UI. A device-code flow
  (`tddy-tools auth login`) would make the feature self-serve. The 5-minute access-token TTL
  (`docs/ft/daemon/session-auth.md`) is why a refresh token is accepted at all.
- **A clone runs at ~2.5 MiB/s, and the wire is the reason.** Measured, not estimated: 150 MiB in
  59 s, byte-identical, by `clones_a_large_repository_with_every_byte_intact`
  (`packages/tddy-daemon/tests/remote_git_livekit_acceptance.rs`, `#[ignore]`d; size it with
  `TDDY_REMOTE_GIT_THROUGHPUT_BYTES`). The earlier worry — that a large clone would outrun SCTP
  buffering because `BidiStreamSender::send` (`packages/tddy-livekit/src/client.rs:357`) has no
  application-level windowing — did **not** materialise: nothing stalls, and raising
  `GIT_FRAME_CHANNEL_CAPACITY` eightfold (8 → 64) changes the rate not at all (59.1 s vs 59.0 s).
  So an ACK/window field on `GitServerFrame` would buy nothing; a materially faster transfer needs a
  different carrier, not a bigger buffer. Worth revisiting only if the LiveKit data channel itself
  gets faster, or if a multi-gigabyte repository makes tens of minutes unacceptable.
- **`token.TokenService/GenerateToken` still lets an *authenticated* caller name any room.**
  The unauthenticated-mint hole is closed: the request carries a `session_token`
  (`packages/tddy-service/proto/token.proto`), the daemon's registration
  (`tddy_daemon::auth::build_token_service_entry`) verifies it with the same resolver that gates
  every other daemon RPC, and the service refuses any `daemon-*` identity on *every* registration
  (`tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX`) so no caller can be admitted as a daemon's
  RPC participant and be handed other participants' calls.
  **What remains:** an authenticated operator may still ask for a JWT for *any room name*, and the
  two `--web-port` registrations plus the LiveKit-surface one in `tddy-coder`
  (`packages/tddy-coder/src/run.rs`) stay unauthenticated — a session coder holds no session-token
  signer (`build_auth_service_entry` builds an *unsigned* `AuthServiceImpl`), so an authenticator
  there would refuse every caller and leave `--web-port` unable to open its own terminal. A caller
  already inside a session's room can therefore mint admission to a *different* room from that
  coder. Deliberately not fixed here: authorizing *which* rooms a given user may join needs a
  room-ownership model that does not exist today (rooms are named ad hoc by the daemon, by session
  spawn and by the presenter path), and inventing one to close this would be a much larger change
  than the impersonation vector warranted. Closing it properly means either giving the coder the
  fleet's verifier, or replacing `token.TokenService` on the web with per-purpose mints that derive
  the room server-side the way `auth.LiveKitTokenService/MintLiveKitToken` does.
- **The concurrent-stream cap is global, not per-user.** `MAX_CONCURRENT_GIT_STREAMS = 16`
  (`packages/tddy-daemon/src/remote_git_service.rs`) bounds the host's exposure but lets one busy
  user starve every other user's clone slot. Per-user would need a keyed map with eviction.
- **A `Serve` stream has no idle deadline.** Deliberate: a legitimate `git-upload-pack` negotiation
  can idle arbitrarily long waiting on the client, so any threshold would be a guess that kills real
  clones. The slot cap above and connection-scoped teardown bound the damage instead. Revisit only
  with evidence of a real stuck-stream class.
- **No peer forwarding.** The client addresses one daemon directly by `daemon_instance_id`. A
  project hosted on a *peer* daemon needs the caller to know which peer; there is no
  `StartSession`-style forwarding hop on this path (see `docs/ft/daemon/livekit-peer-discovery.md`).
- **Git protocol v2 is not negotiated.** Git derives its SSH variant from the command basename;
  `tddy-remote-git-repo` is unknown to it, so git selects variant `simple`, which never passes
  `-o SendEnv=GIT_PROTOCOL`. Sessions run v0/v1 — functionally complete, less efficient on ref
  advertisement. Supporting v2 means implementing the `ssh` variant's option contract.
- **`CliSessionManager::start_terminal` still passes `os_user: None`**
  (`packages/tddy-daemon/src/cli_session_manager.rs:744`), so a started Bash terminal runs as the
  daemon's own identity rather than the session owner's — unlike the main claude terminal, which
  passes `Some(os_user)` (`connection_service.rs:5313`). This changeset surfaces the gap (its own
  git children *do* impersonate, via `wrap_argv_for_privilege_drop`) but does not fix it; on a
  multi-user host the two paths currently disagree about who a spawned process is.

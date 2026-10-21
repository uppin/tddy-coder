# The git shim and the worktree mirror are built by wiring crates - PRD

**Date**: 2026-10-21
**PRD Type**: Technical Improvement
**Stack**: #satellite 14/15. Umbrella PRD:
[PRD-2026-10-16-livekit-satellite-integration.md](./PRD-2026-10-16-livekit-satellite-integration.md)
(#623).

## Affected Features

- **Primary Feature**: [Remote git repo](../remote-git-repo.md). The `tddy-remote-git-repo` binary
  (git's `GIT_SSH_COMMAND` shim) is built by a new wiring crate, `tddy-remote-git-repo-cli`.
- **Primary Feature**: [Session worktree sync](../session-worktree-sync.md). The `tddy-session-sync`
  binary is built by a new wiring crate, `tddy-session-sync-cli`.

## Summary

`tddy-remote-git-repo` and `tddy-session-sync` already join rooms only through the client room port
(`tddy_room_client::RoomConnector`, #629). The one LiveKit edge left in each is its `src/main.rs`,
which builds `tddy_livekit::room_connector::LiveKitRoomConnector`. Cargo has no binary-only
dependency, so that edge sits on the library's manifest, and the satellite ratchet carries both
crates on a special allow-list (`WIRING_WITH_A_LIBRARY`).

This node moves each binary into its own wiring crate, as #630 did for `tddy-coder`
(`tddy-coder-cli`). The library crates stop depending on `tddy-livekit` and stop building a binary.
Each wiring crate depends on its library and on `tddy-livekit`, and binds LiveKit as the room
transport. The binaries keep their names and paths.

## Background

The user asked for these clients to be "defined by RPC boundary", with LiveKit "a possible RPC
transport binding in the wiring binaries, but not as a direct dependency to the libraries". The
boundary already exists:

| Library | Transport-neutral entry point | Port it takes |
|---|---|---|
| `tddy-remote-git-repo` | `relay::run(request, credentials, connector)`; `relay::relay_over_duplex` (stdio ↔ duplex seam) | `Arc<dyn RoomConnector>` → `RoomSession::open_duplex` → `RoomDuplex` |
| `tddy-session-sync` | `attach(credentials, connector)`, then `run(credentials, attached)` | `Arc<dyn RoomConnector>` → `RoomSession::{rpc, subscribe, peer_present}` |

Only the binding is still in the library crates. The library APIs still use some LiveKit names. They
are described in the changeset and are out of scope here.

## Proposed Changes

### What's Changing
- New packages `packages/tddy-remote-git-repo-cli` and `packages/tddy-session-sync-cli`. Each is
  binary-only, `[[bin]]` keeps the original binary name, and `src/main.rs` is the moved entry point.
- The libraries lose their `[[bin]]` target, `src/main.rs`, and the `tddy-livekit` dependency. Any
  dependency only `main.rs` used (`clap`, `env_logger`) goes with it.
- The two suites that run the built binary (`tests/cli_acceptance.rs` in each crate) move to the
  wiring crates. Their assertions do not change.
- `release` and `scripts/dev-runtime-binaries.sh` name the `-cli` packages.
- Ratchet: both `-cli` crates join `WIRING`, both libraries leave `WIRING_WITH_A_LIBRARY`, and a new
  test pins which crate builds each client binary.

### What's Staying the Same
- Binary names, paths, flags, environment variables, exit codes and `--help` text.
- What `./install`, `publish.sh`, CI staging and the VM testkit ship. They list binaries by name, not
  packages.
- The library APIs, including the LiveKit-named fields of `tddy-session-sync`'s credentials.

## Impact Analysis
- **Technical:** two new workspace members. `cargo build -p tddy-remote-git-repo` no longer produces
  the binary; `-p tddy-remote-git-repo-cli` does.
- **User:** none.

## Success Criteria
- `tddy-remote-git-repo` and `tddy-session-sync` name no LiveKit crate, and their `src/` has no
  path into one.
- `cargo metadata` shows the `tddy-remote-git-repo` and `tddy-session-sync` binaries built by the
  `-cli` crates and by nothing else.
- The moved `cli_acceptance` suites pass unchanged in the `-cli` crates.

## Out of Scope
- `tddy-tools` (#satellite 15/15, `feature/satellite/tools-wiring`).
- Renaming the LiveKit-named library surface (`LiveKitCredentials`, `SyncArgs::livekit_*`, the
  `--livekit-*` flags and `LIVEKIT_*` variables, the daemon's `MintLiveKitToken` RPC).
- A build of either binary without LiveKit.

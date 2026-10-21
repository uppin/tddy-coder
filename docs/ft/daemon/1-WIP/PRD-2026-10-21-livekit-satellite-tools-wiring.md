# tddy-tools is built by a wiring crate; the library has no LiveKit feature - PRD

**Date**: 2026-10-21
**PRD Type**: Technical Improvement
**Stack**: #satellite 15/15. Umbrella PRD:
[PRD-2026-10-16-livekit-satellite-integration.md](./PRD-2026-10-16-livekit-satellite-integration.md)
(#623).

## Affected Features

- **Primary Feature**: the `tddy-tools` binary: the agent's tool and MCP binary, `pty-relay`,
  `session-hook`, `restructure`. A new wiring crate, `tddy-tools-cli`, builds it.
- **Related Feature**: [Terminal sessions](../terminal-sessions.md). `tddy-tools pty-relay
  --livekit-url` joins a room through the binary's bound transport.

## Summary

`tddy-tools` reaches rooms only through `tddy_room_client::RoomConnector`. Its `main.rs` installs
the connector once (`session_tool_client::install_room_connector`) and hands it to `pty-relay`.
LiveKit enters through an optional `tddy-livekit` dependency behind a default-on `livekit` feature.
`main.rs` is the one place that names it, and it falls back to `NoRoomTransport` when the feature is
off.

This node does for `tddy-tools` what #satellite 14/15 did for the git shim and the worktree mirror.
A new package, `tddy-tools-cli`, builds the `tddy-tools` binary and always binds
`LiveKitRoomConnector`. The library loses its `[[bin]] tddy-tools`, its `tddy-livekit` dependency
and its `livekit` feature. With that, `WIRING_WITH_A_LIBRARY` is empty, and the ratchet deletes it
and its shape test.

## Background

The user asked for these clients to be "defined by RPC boundary", with LiveKit "a possible RPC
transport binding in the wiring binaries, but not as a direct dependency to the libraries". The
library already has that boundary: `install_room_connector(Arc<dyn RoomConnector>)` for every split
session's tool, roster and conversation call, and `pty_relay::run_pty_relay(args, connector)` for the
terminal relay. The `livekit` feature puts the binding back into the library's manifest.

## Proposed Changes

### What's Changing
- New package `packages/tddy-tools-cli`. It is binary-only, has `[[bin]] name = "tddy-tools"`, and
  its `src/main.rs` is the moved entry point, which binds `LiveKitRoomConnector` unconditionally.
- The six modules that only `main.rs` declares (`cli`, `index_client`, `index_console`, `pty_relay`,
  `remote_cli`, `session_hook`) become modules of the `tddy-tools` library, which the wiring crate
  calls.
- The library loses `[[bin]] tddy-tools`, `tddy-livekit`, and `[features] default = ["livekit"]` /
  `livekit`. It keeps the `execute-tool-stdio-fixture` binary, a test fixture for a library suite.
- The 35 test suites that run the built binary move to `tddy-tools-cli/tests`. Their assertions do
  not change.
- `release`, `test`, `claude-sandbox`, `run-index-daemon`, `scripts/dev-runtime-binaries.sh`, the
  `tddy-coder` "binary not found" hint and `packages/tddy-tools/README.md` name `tddy-tools-cli`
  where they build.
- Ratchet: `tddy-tools-cli` joins `WIRING`, `tddy-tools` joins `CLIENT_LIBRARIES` and
  `CLIENT_BINARIES`, and `WIRING_WITH_A_LIBRARY` and its shape test are deleted.

### What's Staying the Same
- The binary's name, path, subcommands, flags, environment and exit codes.
- Every binary list (`install`, `publish.sh`, CI staging, the VM testkit, the e2e fixtures). They name
  binaries, not packages.
- The 12 suites in other packages that run `tddy-tools`. They resolve it as
  `target/debug/tddy-tools` by name.

## Impact Analysis
- **Technical:** `cargo build -p tddy-tools` no longer produces the binary;
  `cargo build -p tddy-tools-cli` does. `tddy-daemon-sandbox`'s dev-dependency on the library no
  longer drags in the LiveKit SDK.
- **User:** none. The `--no-default-features` build of a binary without LiveKit is gone. Nothing in
  the repo uses it.

## Success Criteria
- No library crate names a LiveKit crate, and the ratchet has no exception list for one that ships a
  binary.
- `cargo metadata` shows `tddy-tools` built by `tddy-tools-cli` alone.
- The moved suites pass unchanged in `tddy-tools-cli`, and the library suites still pass in
  `tddy-tools`.

## Out of Scope
- Decomposing `server.rs` or `cli.rs`. Code issues and todo entries are recorded in the changeset.
- Moving `execute-tool-stdio-fixture` out of the library manifest
  (`docs/dev/todo/2026-09-10-…`).
- A LiveKit-free build of the binary.

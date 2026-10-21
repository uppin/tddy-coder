# tddy-session-sync-cli

Builds the **`tddy-session-sync`** binary, which mirrors a tddy session's worktree locally. The
binary keeps its name and path (`target/{debug,release}/tddy-session-sync`); only the package that
builds it is called `-cli`.

This package is the wiring crate. `src/main.rs` parses the command line and hands
`tddy_livekit::room_connector::LiveKitRoomConnector` to the library's `attach(credentials, connector)`,
then runs the mirror. Credentials, the room join, the sync loop and the git commands live in
[`tddy-session-sync`](../tddy-session-sync/README.md), which names no LiveKit crate and builds no binary.

```bash
cargo build --release -p tddy-session-sync-cli    # what ./release runs
```

`tests/cli_acceptance.rs` runs the built binary, including that it never prints the value of a token
it found in the environment. It lives here because `CARGO_BIN_EXE_*` is defined only in the package
that owns the binary.

Feature: [docs/ft/daemon/session-worktree-sync.md](../../docs/ft/daemon/session-worktree-sync.md);
module documentation: [mirroring](../tddy-session-sync/docs/mirroring.md). The rule that keeps
LiveKit out of the library:
[satellite ratchet](../tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet).

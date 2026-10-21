# tddy-remote-git-repo-cli

Builds the **`tddy-remote-git-repo`** binary: git's `GIT_SSH_COMMAND` shim that clones a daemon
project over a LiveKit room. The binary keeps its name and path (`target/{debug,release}/tddy-remote-git-repo`);
only the package that builds it is called `-cli`.

This package is the wiring crate. `src/main.rs` parses the command line and hands
`tddy_livekit::room_connector::LiveKitRoomConnector` to the library's
`relay::run(request, credentials, connector)`. The relay itself, the credentials and the
`ssh` command-line parsing live in [`tddy-remote-git-repo`](../tddy-remote-git-repo/README.md), which
names no LiveKit crate and builds no binary.

```bash
cargo build --release -p tddy-remote-git-repo-cli    # what ./release runs
```

`tests/cli_acceptance.rs` runs the built binary: its `--help` text, and the exit code of a rejected
command line. It lives here because `CARGO_BIN_EXE_*` is defined only in the package that owns the
binary, and because a workspace `cargo test` therefore builds the binary that the `tddy-worktree-service`
and `tddy-session-lifecycle` suites resolve beside their own executable.

Feature: [docs/ft/daemon/remote-git-repo.md](../../docs/ft/daemon/remote-git-repo.md). The rule that
keeps LiveKit out of the library:
[satellite ratchet](../tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet).

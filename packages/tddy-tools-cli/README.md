# tddy-tools-cli

Builds the **`tddy-tools`** binary: the agent's tool and MCP binary (`--mcp`, `submit`, `ask`,
`pty-relay`, `session-hook`, `restructure`, …). The binary keeps its name and path
(`target/{debug,release}/tddy-tools`); only the package that builds it is called `-cli`, so agents,
the daemon, the sandbox and every script that resolves the binary by name are unaffected.

This package is the wiring crate. `src/main.rs` parses the command line, installs
`tddy_livekit::room_connector::LiveKitRoomConnector` once into
`tddy_tools::session_tool_client::install_room_connector`, hands the same connector to
`tddy_tools::pty_relay::run_pty_relay`, and dispatches every subcommand into the library's command
modules (`cli`, `index_client`, `pty_relay`, `remote_cli`, `session_hook`) or to the crate that owns
it (`tddy_bsp`, `tddy_code_analysis`, `tddy_code_restructuring`). It binds LiveKit unconditionally:
there is no `livekit` feature and no build without a room transport. Everything else lives in
[`tddy-tools`](../tddy-tools/README.md), which names no LiveKit crate.

```bash
cargo build -p tddy-tools-cli     # what ./release, ./test, ./claude-sandbox and ./run-index-daemon build
cargo test -p tddy-tools-cli      # the 35 suites that run the built binary
```

The suites in `tests/` drive the binary from outside (`assert_cmd`, the real `--mcp` stdio wire).
They live here because `cargo_bin_cmd!` / `CARGO_BIN_EXE_tddy-tools` resolve only in the package that
builds the binary; suites in other packages find it as `target/debug/tddy-tools`, which `./test`
prebuilds with `-p tddy-tools-cli`.

The rule that keeps LiveKit out of the library:
[satellite ratchet](../tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet).

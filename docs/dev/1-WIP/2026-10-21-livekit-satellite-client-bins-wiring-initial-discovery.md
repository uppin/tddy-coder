# Initial Discovery: #satellite 14/15 — client-bins-wiring

**Changeset**: [2026-10-21-livekit-satellite-client-bins-wiring.md](./2026-10-21-livekit-satellite-client-bins-wiring.md)
**Date**: 2026-10-21
**Passes**: 1 (whole work for nodes 14 and 15; node 15 copies it as its Exploration 1)

## Combined Conclusions

- The three client crates reach rooms only through `tddy_room_client::RoomConnector` (#629). The
  only LiveKit edge in each is its `src/main.rs` building
  `tddy_livekit::room_connector::LiveKitRoomConnector`:
  `tddy-remote-git-repo/src/main.rs:14,86`, `tddy-session-sync/src/main.rs:15,124`, and
  `tddy-tools/src/main.rs:133-142` (`#[cfg(feature = "livekit")]`, otherwise `NoRoomTransport`).
  No library source and no test in any of the three names `tddy_livekit` (grep, 2026-10-21).
- The ratchet allows the three on `WIRING_WITH_A_LIBRARY`
  (`packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs:56`). The shape test
  `a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary` holds every other source
  file clean. `LIVEKIT_PATHS` (`livekit::`, `livekit_api::`, `livekit_protocol::`) is matched as a
  substring, so it also catches `tddy_livekit::`.
- #630's `tddy-coder-cli` is the model. A new package keeps `[[bin]] name = "tddy-coder"`, the
  library loses the bin and `tddy-livekit`, the tests that run the binary move (renames with zero
  content change), `release`, `test`, `scripts/dev-runtime-binaries.sh` and nextest filters name the
  `-cli` package, and the ratchet adds it to `WIRING`. #630's contract commit published no `-cli`
  package; its red test was the ratchet list edit.
- `WorkspaceGraph` (`tddy-testing-commons/src/dependency_graph.rs`) reads
  `cargo metadata --no-deps`, which also lists each package's `targets` (`kind: ["bin"]`). A
  `crates_building(binary)` method lets the ratchet assert which crate builds a binary, with no stub
  package.

### tddy-remote-git-repo / tddy-session-sync

- Tests that run the binary: only `tests/cli_acceptance.rs` in each, through
  `env!("CARGO_BIN_EXE_<bin>")`. All other suites drive the library with
  `tddy-room-client`'s `in_memory` connector.
- Cross-package users resolve the git shim **by name beside the test executable**:
  `tddy-worktree-service/tests/remote_git_livekit_acceptance.rs:176` and
  `tddy-session-lifecycle/tests/session_sync_livekit_acceptance.rs:740`.
  `tddy-projects/src/project_provision.rs:24` tries `CARGO_BIN_EXE_tddy-remote-git-repo`, then the
  sibling, then `PATH`. None names the package, so the move changes nothing for them, provided the
  binary is still built in a workspace run. The `-cli` crate's own test target guarantees that.
- Package lists that build them: `release:67,69`, `scripts/dev-runtime-binaries.sh:27,29`. Binary
  lists (unchanged): `install:384,937`, `publish.sh:64,124,126`, `.github/workflows/ci.yml:563,599`,
  `tddy-vm-testkit/src/recipes.rs:125,127`, and the three `tddy-e2e` fixtures.
- Library API still LiveKit-named: session-sync's `LiveKitCredentials`, `Credentials::livekit`,
  `SyncArgs::livekit_*`, and the `LIVEKIT_*` variables. remote-git-repo calls the daemon's
  `MintLiveKitToken`.

### tddy-tools (node 15)

- `src/main.rs` (232 lines) declares six **binary-only** modules that the library does not contain:
  `cli` (910), `index_client` (525), `index_console` (557), `pty_relay` (225), `remote_cli` (357) and
  `session_hook` (374), 2,948 lines in all; five of them carry inline `#[cfg(test)]` tests. `cli.rs` hosts the
  `complexity-cli-run-call-tool` code issue.
- 35 of 54 test files run the binary (`env!("CARGO_BIN_EXE_tddy-tools")` or
  `assert_cmd::cargo_bin_cmd!("tddy-tools")`). 19 are library tests. The
  `execute-tool-stdio-fixture` `[[bin]]` serves `tests/session_tool_stdio_rpc_dispatch.rs`, a
  library test.
- 12 suites in other packages find `tddy-tools` through
  `var_os("CARGO_BIN_EXE_tddy-tools").unwrap_or(target/debug/tddy-tools)`, a path by binary name.
  `tddy-daemon-sandbox/src/sandbox_session.rs:557` does the same in production.
- `tddy-daemon-sandbox` dev-depends on the `tddy-tools` library, with default features, so on the
  `livekit` feature.
- Package lists: `release:61`, `test:25`, `claude-sandbox:93`, `run-index-daemon:25,118,221,333`,
  `scripts/dev-runtime-binaries.sh:24`. Text: `packages/tddy-coder/src/run.rs:197`,
  `packages/tddy-tools/README.md:11,23`, `.agents/skills/darwin-sandbox/SKILL.md:67` (already
  stale: those suites live in `tddy-sandbox-darwin`), `docs/ft/coder/warm-code-intelligence-daemon.md:166`,
  and `packages/tddy-workflow-recipes/docs/workflow-schemas.md:33` (package doc: wrap only).
- Nothing builds `tddy-tools` with `--no-default-features`. The `livekit` feature's only consumer
  is its own `main.rs`.

## Exploration 2 — the contract (2026-10-21)

- Baseline on `2fdcc7481` (re-verified on `c2a84c343` after the rebase): `cargo test -p tddy-daemon-extensions --test livekit_is_a_satellite`
  passes 5/5. #635 has already closed the ratchet.
- After the list edit and the two new tests, three fail: `only_plugins_and_wiring_name_a_livekit_crate`
  (it lists the two libraries), `a_client_library_names_livekit_nowhere_in_its_sources` (it lists the two
  `src/main.rs`), and `each_client_binary_is_built_by_its_wiring_crate` (both binaries are built by
  their library crates).
- Found for node 15: `packages/tddy-tools/BUILD.yaml` names `package: tddy-tools` for the BSP
  `rust_binary` target (`cargo build -p <package> --bin <bin_name>`, in `tddy-build-rust`). #630 left
  `packages/tddy-coder/BUILD.yaml` pointing at `package: tddy-coder`, which has had no binary since
  then. The two client crates here have no `BUILD.yaml`.

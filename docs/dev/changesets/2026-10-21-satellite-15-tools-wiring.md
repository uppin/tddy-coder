# 2026-10-21 — tddy-tools is built by a wiring crate; no library names LiveKit

**Type:** Architecture

`#satellite` 15/15 ([#690](https://github.com/uppin/tddy-coder/pull/690)), the stack's top. No
library crate names a LiveKit crate, not even optionally, and the ratchet keeps no exception list
for a library that ships a binary. Every client binary is built by a wiring crate that binds
`LiveKitRoomConnector` to a library's `RoomConnector` boundary.

## What changed

- **New package `tddy-tools-cli`.** Binary-only, `[[bin]] name = "tddy-tools"`. `src/main.rs`
  moved from `tddy-tools` (R096: its six `mod` lines became one `use tddy_tools::{…}`, and
  `room_connector()` lost its `cfg(feature = "livekit")` pair and the `NoRoomTransport` arm). The 35
  suites that run the binary moved with it, as renames; two comment lines in them were repointed to
  the new package.
- **`tddy-tools` library.** Lost `[[bin]] tddy-tools`, `tddy-livekit`, the `livekit` feature and
  the dependencies only `main.rs` used. The six binary-only modules (`cli`, `index_client`,
  `index_console`, `pty_relay`, `remote_cli`, `session_hook`) are now `pub mod`s of `lib.rs`.
  `cargo tree -p tddy-tools -e normal,build | grep -ci livekit` → 0, so `tddy-daemon-sandbox`'s
  dev-dependency no longer pulls the LiveKit SDK.
- **Scripts and build.** `release`, `test`, `claude-sandbox`, `run-index-daemon` and
  `scripts/dev-runtime-binaries.sh` build `-p tddy-tools-cli`. `packages/tddy-tools/BUILD.yaml`
  names `package: tddy-tools-cli` with both crates in `srcs`; `packages/tddy-coder/BUILD.yaml` names
  `package: tddy-coder-cli` (a defect left by #630; its `srcs` are recorded in the backlog).
  Binary-name lists (`install`, `publish.sh`, CI staging) are unchanged.
- **Ratchet.** `tddy-tools-cli` joins `WIRING`; `tddy-tools` joins `CLIENT_LIBRARIES` and
  `CLIENT_BINARIES`; `WIRING_WITH_A_LIBRARY` and
  `a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary` are deleted
  ([extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet)).
  6/6 green.
- **Text.** The `tddy-coder` "binary not found" hint, `.agents/skills/darwin-sandbox/SKILL.md`,
  `docs/ft/coder/warm-code-intelligence-daemon.md`, `packages/tddy-workflow-recipes/docs/workflow-schemas.md`
  and two stale doc comments (`tddy-integration-tests`, `tddy-workflow-recipes`) name the new package.

## Engine gap and hand edits

The restructure engine has no operation for a module declared in a binary root, nor for moving a
binary root or a test crate between packages. With the developer's consent the six modules were
re-parented by hand and `main.rs` and the 35 suites were `git mv`'d. Each hand fix carries a
`TODO(restructure-engine)` marker. Outside this changeset's scope, at the developer's request,
comment-only `TODO(restructure-engine)` markers were added at the three refusal points in
`tddy-code-restructuring` (`module_reparent/survey.rs`, `crate_move/module_home.rs`,
`item_anchor.rs`). Backlog: `2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root`
(new).

## Backlog

- `2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin`: answered for the
  client binaries (no feature on the wiring crates); the entry stays for the daemon.
- `2026-10-08-tddy-tools-server-rs-is-3-9k-lines-and-the-crate-mixes-several-responsibilities`:
  ⚠ DURING, not fixed: the binary's wiring left the crate, the crate split did not happen; kept unchanged.
- New: `2026-10-21-tddy-coder-build-yaml-srcs-miss-the-cli-crate`.

## Known over-budget files (not grown here)

`packages/tddy-tools/src/server.rs` (`oversized-file-server`, unclaimed, left to the sweep node),
`packages/tddy-tools/src/cli.rs` (895, new record `oversized-file-cli`),
`packages/tddy-coder/src/run.rs` (2,655, `oversized-file-run`, one string changed).

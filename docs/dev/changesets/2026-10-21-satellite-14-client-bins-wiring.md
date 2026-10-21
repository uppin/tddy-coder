# 2026-10-21 — The git shim and the worktree mirror are built by wiring crates

**Type:** Architecture

`#satellite` 14/15 ([#689](https://github.com/uppin/tddy-coder/pull/689)). `tddy-remote-git-repo` and
`tddy-session-sync` are libraries that name no LiveKit crate; each binary is built, under its
original name and path, by a wiring crate that binds `LiveKitRoomConnector` to the library's
`RoomConnector` boundary.

## What changed

- **New packages.** `tddy-remote-git-repo-cli` and `tddy-session-sync-cli` are binary-only. Each
  `[[bin]]` keeps the original name; `src/main.rs` and `tests/cli_acceptance.rs` moved from the
  library with no content change (`git diff -M`: R100). A workspace `cargo test` builds each binary
  through that suite, which is what the `tddy-worktree-service` and `tddy-session-lifecycle` suites that run the
  git shim rely on.
- **Libraries.** Both lost `[[bin]]`, `tddy-livekit` and `env_logger`.
- **Scripts.** `release` builds `-p tddy-remote-git-repo-cli -p tddy-session-sync-cli`;
  `scripts/dev-runtime-binaries.sh` lists the two packages. Binary-name lists (`install`,
  `publish.sh`, CI staging) are unchanged.
- **Ratchet.** The two `-cli` crates join `WIRING`; the libraries leave `WIRING_WITH_A_LIBRARY`;
  two tests are new ([extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet)).
  `WorkspaceGraph::crates_building` reads `bin` targets
  ([dependency graph](../../packages/tddy-testing-commons/docs/dependency-graph.md)); `read` was split into
  two helpers (70 to 22 lines).

## Not changed

The LiveKit-shaped library surface stays: `LiveKitCredentials`, `SyncArgs::livekit_*`, the `LIVEKIT_*`
variables, and the daemon's `MintLiveKitToken` RPC that the relay calls. The library boundary is
transport-neutral (`relay::run`, `attach` take an `Arc<dyn RoomConnector>`), but `RoomAuth::Credentials`
presumes a transport whose server signs tokens from an API key pair.

## Engine gap

The restructure engine refused both moves (a binary root and a test-crate file), so they were
`git mv`'d with the developer's consent and proved byte-identical. The gap is recorded in the backlog
as `2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate`.

## Known over-budget files (unchanged here)

`tddy-session-sync`'s `attach.rs` (555 production lines) and `sync.rs` (788) stay over the 500-line
budget; this change did not touch them and their code-issue records stand.


# 2026-10-21 — The ratchet holds each client binary to its wiring crate

**Type:** Architecture

`#satellite` 14/15 ([#689](https://github.com/uppin/tddy-coder/pull/689)).

`tddy-remote-git-repo-cli` and `tddy-session-sync-cli` join `WIRING`; `tddy-remote-git-repo` and
`tddy-session-sync` leave `WIRING_WITH_A_LIBRARY` (now `tddy-tools` alone). Two tests are new:
`a_client_library_names_livekit_nowhere_in_its_sources` reads every source file of the two libraries,
and `each_client_binary_is_built_by_its_wiring_crate` reads which crate builds each client binary.

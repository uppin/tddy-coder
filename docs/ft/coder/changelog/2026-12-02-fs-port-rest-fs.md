# 2026-12-02 — Every remaining production crate goes through the Fs port

The production disk I/O of the remaining crates (`tddy-coder`, `tddy-tools`,
`tddy-presenter`, `tddy-telegram-control`, `tddy-toolcall`, `tddy-service`,
`tddy-model-registry`, `tddy-tui`, `tddy-livekit`, `tddy-screen-sharing`, `tddy-graph`,
`tddy-acp-stub`, `tddy-telegram`, `tddy-terminal-rpc`, `tddy-vm-build`,
`tddy-tools-cli`, `tddy-session-sync-cli`, `tddy-livekit-screen-capture`,
`tddy-coder-cli`, `tddy-work-queue`, `tddy-sandbox-qemu`, `tddy-build`) goes through
the `tddy-fs` port (refactor, no behaviour change), behind held handles, `<fn>_on`
seams and an `AsyncFs` with `DiskFs` only at composition roots. `tddy-zed-extensions`
is a recorded known gap with a `docs/dev/todo/` follow-up.

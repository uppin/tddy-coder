# 2026-11-08 — Rename the Rust `*_ghostty_virtual_terminal_e2e` tests

**Category:** Naming leftover
**Source:** `#xterm` 4/4 (PR #869) removed `ghostty-web` and renamed the web terminal components.

`packages/tddy-e2e/tests/grpc_terminal_rpc.rs` (`grpc_ghostty_virtual_terminal_e2e`) and
`packages/tddy-e2e/tests/livekit_terminal_rpc.rs` (`livekit_ghostty_virtual_terminal_e2e`, its room name
`ghostty-virtual-terminal-e2e` and the `ghostty-init` drain labels) still carry the old name.

They test the PTY over RPC, not the web component, so the web rename left them alone. Rename them to
`*_virtual_terminal_e2e` (and update CI test filters and any nextest serial-group entries that name them)
in a Rust-side change, so a rename of a test id does not hide inside a web PR.

Other Rust mentions (`tddy-discovery`, `tddy-session-lifecycle`, `tddy-task`, `tddy-terminal-rpc`,
`tddy-tui-testkit`) are outside the JS workspaces and were not touched.

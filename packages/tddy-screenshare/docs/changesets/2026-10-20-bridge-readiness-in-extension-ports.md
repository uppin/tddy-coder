# 2026-10-20 — The readiness protocol moves to the extension ports

**Type:** Architecture

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)); correction to
`2026-10-20-bridge-ready`. `readiness.rs` and `docs/bridge-readiness.md` move to
`tddy-daemon-extensions` (`bridge_readiness`). The crate depends on it, `run_bridge` writes the
line through it, and the crate root re-exports `BridgeReadiness`, `FailureKind`,
`write_readiness`, `read_readiness` and `AuthenticationRefused` at the same paths. See
[Bridge start report](../../../tddy-daemon-extensions/docs/bridge-readiness.md); cross-package
record: [2026-10-20-bridge-readiness-in-extension-ports.md](../../../../docs/dev/changesets/2026-10-20-bridge-readiness-in-extension-ports.md).

# 2026-10-20 — The bridge readiness protocol joins the extension ports

**Type:** Architecture

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)); correction to
`2026-10-20-bridge-ready`. `bridge_readiness` (`BridgeReadiness`, `FailureKind`,
`write_readiness`/`read_readiness`, `AuthenticationRefused`, and the unit tests
`a_ready_line_round_trips` and `a_failure_line_names_its_kind`) moves here from
`tddy-screenshare`, unchanged, so `tddy-screen-sharing` reads a bridge's start report without
naming a LiveKit crate. The crate gains `serde` and `serde_json`. See
[Bridge start report](../bridge-readiness.md); cross-package record: [2026-10-20-bridge-readiness-in-extension-ports.md](../../../../docs/dev/changesets/2026-10-20-bridge-readiness-in-extension-ports.md).

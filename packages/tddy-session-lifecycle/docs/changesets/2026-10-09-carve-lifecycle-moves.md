# 2026-10-09 — Lifecycle is a wiring crate

**Type:** Architecture

Every topic module moved into a receiver crate (`tddy-agent-launch`, `tddy-session-split`,
`tddy-cli-sessions`, `tddy-demo-vm-service`, `tddy-session-agents`, `tddy-session-files`,
`tddy-session-activity`, `tddy-daemon-livekit`, `tddy-daemon-kernel`), leaving the host, the builders, the
port impls and `PeerRouted*`. About 20.3k production lines became 5.6k in 38 files; the largest,
`connection_service.rs`, is 525 (from 582). Every moved module keeps its `tddy_session_lifecycle::…` path
through a facade, so the consumers are unedited apart from the two `DemoVmServiceImpl::new(state)` call sites
in `tddy-daemon`. Tests: 658 to 564 passed (the 22 known failures unchanged by name); the 94 that moved run in
the receivers. The layout is in [module-layout.md](../module-layout.md); the cross-package entry with the
per-crate numbers is [`2026-10-09-carve-lifecycle-moves`](../../../../docs/dev/changesets/2026-10-09-carve-lifecycle-moves.md).
Not done, approved: `test_util` (366 lines) is not gated behind a feature.

# 2026-10-04 — The testkit's LiveKit image is pinned in one file

**Type:** Refactor

`#e2e-leg` 4/5, PR [#581](https://github.com/uppin/tddy-coder/pull/581). Cross-package entry:
[2026-10-04-e2e-leg-shared-livekit-ci.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-shared-livekit-ci.md).
State B: [livekit-testkit.md](../livekit-testkit.md).

`LIVEKIT_IMAGE` and `LIVEKIT_TAG` became `LIVEKIT_IMAGE_REF`, compiled in with `include_str!` from
`.config/livekit-server.image` (`livekit/livekit-server:v1.13.7`) and split into name and tag at run
time. Previously the testkit asked for the floating `:master` tag; a bare `cargo test` with no
`LIVEKIT_TESTKIT_WS_URL` still launches its own container, now of the pinned image.

# 2026-08-13 — `TokenGenerator::generate_for` performs no authorization

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-13

`packages/tddy-livekit/src/token.rs:50-65` mints a JWT for whatever `(room, identity)` pair it is
handed, with no check that the caller may join that room or claim that identity. Authorization now
lives one layer up, in `token.TokenService` itself: the daemon's registration demands a verified
`session_token`, and no registration will mint a `daemon-*` identity. A *session coder's*
`token.TokenService` HTTP endpoint is still a room-agnostic minting oracle for anything that can
reach it, and even on the daemon an authenticated caller may name any room — see the
remote-git-repo-over-livekit entry above for why, and what closing it would take. Default TTL is 6 h
(`DEFAULT_LIVEKIT_JWT_TTL_SECS`). Separately, `spawner.rs:886-902` passes the raw
`--livekit-api-secret` on the spawned child's command line, where `/proc/<pid>/cmdline` exposes it to
the spawning user.

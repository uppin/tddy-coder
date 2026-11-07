# 2026-11-07 — Web checks do not run when the Rust binaries they exec change

**Category:** Missing test trigger (CI gate)
**Source:** `#web-split` 1/16 (PR #838), `/validate-changes`

Two web checks build and exec a Rust binary: `web / tddy-livekit-rpc-web tests` runs
`examples/echo_server` (crate `tddy-livekit`) under Cypress, and `web / tddy-rust-typescript-tests tests`
spawns `tddy-coder` (`tddy-coder-cli` and everything it links). `scripts/web-affected.ts` only looks at
JS workspace directories and global inputs, so a Rust-only PR that changes either binary's behaviour
runs neither check. The old single `Web tests` job ran `tddy-livekit-rpc-web`'s Cypress on every PR.

**Why deferred:** the PRD for #838 intends Rust-only PRs to skip web checks; deciding which Rust paths
should re-run which web package is a design call, and `tddy-coder` reaches most of the workspace.

**What closing it takes:** a per-package list of extra input paths in `scripts/web-affected.ts` (e.g.
`tddy-livekit-rpc-web` → `packages/tddy-livekit/`, `packages/tddy-rpc/`), driven from the caller workflow
or the script, with a unit test per entry; or running those two checks on every PR and accepting the
cold Rust build.

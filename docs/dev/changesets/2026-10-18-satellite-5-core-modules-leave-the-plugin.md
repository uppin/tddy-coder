# 2026-10-18 — LiveKit-free modules leave the LiveKit plugin

**Type:** Architecture (mechanical move)

`#satellite` 5/13 ([#627](https://github.com/uppin/tddy-coder/pull/627)). `tddy-daemon-livekit` declares
only LiveKit work: three modules that name no LiveKit crate moved to their consumers, and two crates
dropped a `livekit` dependency no source used.

## What changed

- **`placement` and `session_git`** → `tddy-agent-launch` (their only consumer). The `session_git`
  suite moved with it, to `tests/session_git_identity.rs`.
  [Module layout](../../packages/tddy-agent-launch/docs/module-layout.md#placement-and-the-sessions-git-environment).
- **`os_user_resolution`** → `tddy-daemon-kernel`, as its own module.
  [Daemon kernel](../../packages/tddy-daemon-kernel/docs/daemon-kernel.md#os_user_resolution--a-session-token-to-its-os-user).
- **Identity imports.** The `local_instance_id_for_config` users import it from
  `tddy_daemon_kernel::daemon_identity`, where it is defined, not through the plugin's re-export.
- **`tddy-session-lifecycle`** re-exports `placement` and `os_user_resolution` from their new homes.
- **Manifests.** `livekit = "0.7"` left `[dependencies]` of `tddy-daemon-rpc` and
  `tddy-session-lifecycle`. `tddy-daemon-rpc` keeps it as a dev-dependency for the session-room
  exec-tool suite.
- **Ratchet.** `livekit_is_a_satellite.rs` asserts neither crate names the SDK;
  `plugin_holds_only_livekit_work.rs` asserts the plugin declares none of the three modules.

## Not changed

No port, no behaviour, no `KNOWN_OFFENDERS` entry: every crate touched still names the plugin for LiveKit
work, which later nodes move.

## How it was made

Four `tddy-tools restructure` operations (`move_test_binary_to_crate`, then three
`move_module_to_crate`); no module was moved by hand. Three applies ended with a tree that did not
compile, and each was fixed forward by hand: the engine wrote `tddy-agent-launch = { path = "" }`
into the destination's own manifest (a self-dependency), rewrote same-crate callers to
`tddy_agent_launch::…` instead of `crate::…`, and left one grouped import. It also does not rewrite
string-literal paths, which the relocated test had one of, and its applies skipped rustfmt.

## Verification

Scoped to the eleven packages touched: `cargo clippy --all-targets -D warnings` is clean, the two shape
tests, the relocated suite and the rest of the ratchet suite pass, and `cargo fmt --all --check` is clean.
Fifteen sandbox-bridge tests fail on macOS ("sandbox RPC bridge not installed") as they do without this
change. `tddy-coder`'s own suite was not run (a doc comment changed there).

## Code issues

No file grew. `oversized-file-runtime` 1,820 → 1,818; `oversized-file-project-coordinate-handlers` 660 →
660 (touched, unchanged). New records, alert-only: `oversized-file-session-coordinate-handlers`
(`tddy-agent-launch`, 503) and `oversized-file-host-registry` (`tddy-host-service`, 596). The three
`## Prerequisites` entries were touched, not resolved.

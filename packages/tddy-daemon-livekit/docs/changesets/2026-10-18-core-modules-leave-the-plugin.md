# 2026-10-18 — Core modules leave the plugin

**Type:** Architecture (mechanical move)

`placement`, `session_git` and `os_user_resolution` no longer live in `tddy-daemon-livekit`: none named
LiveKit. The first two are `tddy-agent-launch` modules, the third a `tddy-daemon-kernel` module. The
crate's `lib.rs` declares only LiveKit work, which `tests/plugin_holds_only_livekit_work.rs` asserts.
`session_git_identity_acceptance.rs` moved with `session_git`.

Cross-package record: [`docs/dev/changesets/2026-10-18-satellite-5-core-modules-leave-the-plugin.md`](../../../docs/dev/changesets/2026-10-18-satellite-5-core-modules-leave-the-plugin.md).

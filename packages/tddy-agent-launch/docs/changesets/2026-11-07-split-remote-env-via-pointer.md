# 2026-11-07 — Split agents' pointers are placed through the launch's helpers

**Type:** Feature

`#agent-tools` 25/28 ([#833](https://github.com/uppin/tddy-coder/pull/833)).
`split_session_pointers::HostSplitSessionPointers` implements `tddy-session-split`'s
`SplitSessionPointers` port with #829's `place_session_pointer` / `removing_session_pointer_on_exit`
(local host only), and `remove`s a pointer when a spawn fails. The generic resume places a split
session's own `SplitAgentWiring.session_pointer` and refuses a split session with none.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-split-remote-env-via-pointer.md) and
[module-layout.md](../module-layout.md).

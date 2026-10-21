# 2026-10-21 — A bridge has an inspect mode

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `ScreenSharingClient::inspect(host, port)`. `inspect.rs` adds
`BridgeMode::from_args` (no arguments bridges, `--inspect` inspects, and anything else is refused
by name) and `run_inspect::<C>`, and re-exports `tddy_daemon_extensions::desktop_inspect`. See
[Desktop inspect line](../../../tddy-daemon-extensions/docs/desktop-inspect.md).

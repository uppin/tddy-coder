# 2026-10-20 — X11 displays are listed as desktop channels

**Type:** Feature

`#host-desktops` 7/8 ([#659](https://github.com/uppin/tddy-coder/pull/659)). `x11_displays.rs`
lists the `X<N>` sockets in `screen_sharing.x11_socket_dir` (default `/tmp/.X11-unix`) that accept a
connect from the daemon's user, and `with_x11_channels` appends one `DesktopChannel::X11Display(N)`
reading per display not already served on `5900 + N`, with `can_bridge` requiring `x11vnc` too.
`SubprocessHostToolingProbe::for_config` wires it in through `listing_x11_displays_for`.
`DesktopReachability` gains `channel`, and `host_remote_desktop_message` sends `channel_id`, `kind`,
`display` and `label`. `HostDesktopTarget` gains `display: Option<u32>` (serde default, skipped
when absent). `binary_is_present` is `pub(crate)`. `host_tooling.rs` grew 775 → 806 production
lines (`oversized-file-host-tooling.md`).

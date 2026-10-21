# 2026-10-20 — An X11 channel runs x11vnc around its bridge

**Type:** Feature

`#host-desktops` 7/8 ([#659](https://github.com/uppin/tddy-coder/pull/659)). `x11vnc.rs`:
`X11vncServers` (start on a free loopback port and wait until it accepts, with
`DEFAULT_X11VNC_ACCEPT_TIMEOUT` = 10 s or `with_x11vnc_accept_timeout`; stop; reap),
`x11vnc_arguments`, `X11VNC_HOST`. `start_host_stream` serves an X11 target through it
(`serve_x11_display_to_the_bridge`, `spawn_host_bridge`). It terminates the previous bridge before
the x11vnc. `stop_host_stream` and `reap_bridge` (only for a bridge still named by its key) stop the
x11vnc. `add_host_target` stores `x11_display`, and `host_target_to_proto` returns it. Tests:
`tests/x11_channels_acceptance.rs` (9), with stand-in `x11vnc` and bridge variants in `tests/common`,
both warmed before any test clock starts.

# 2026-10-20 — One desktop channel per X11 display, served through x11vnc

**Type:** Feature

`#host-desktops` 7/8 ([#659](https://github.com/uppin/tddy-coder/pull/659)), across `tddy-service`
(protos), `tddy-host-service`, `tddy-daemon-kernel`, `tddy-screen-sharing` and `tddy-web`.

- `host.proto` `HostRemoteDesktop` gains `channel_id`, `kind` (`DesktopChannelKind`: `ENDPOINT`,
  `X11_DISPLAY`), `display`, `label`. `screen_sharing.proto` `AddHostTargetRequest` and
  `ScreenSharingTarget` gain `optional uint32 x11_display`. Generated TS committed in both `gen/`
  directories.
- `tddy-host-service`: `x11_displays.rs` (`list_x11_displays`, `DEFAULT_X11_SOCKET_DIR`,
  `with_x11_channels`), `DesktopChannel` on every reading, `HostDesktopTarget.display`.
- `tddy-daemon-kernel`: `ScreenSharingConfig.x11vnc_binary_path`, `.x11_socket_dir`,
  `resolve_x11vnc_binary_path`.
- `tddy-screen-sharing`: `x11vnc.rs` (`X11vncServers`), run around the bridge in `start_host_stream`
  and stopped on stop, on bridge exit and before a reopen.
- `tddy-web`: one Connect per channel (`connectableChannels`, `ChannelReading`); the overlay opens the
  channel it is given (`x11Display`).

Found and fixed in validation: a reopen stopped the old x11vnc before the old bridge, so the old
bridge's reaper stopped the new x11vnc. The old bridge is now terminated first, and
`reopening_an_x11_channel_whose_old_bridge_exits_with_its_x11vnc_keeps_the_new_x11vnc` pins it.
`an_x11vnc_that_never_accepts_is_stopped` was load-sensitive and is fixed: its stand-in is warmed
before the clock starts, and the accept window is 3 s.

Deferred, each with a `docs/dev/todo/` entry dated 2026-10-20:
`x11-socket-dir-read-failure-lists-no-channels`,
`endpoint-channel-targets-matched-by-loopback-port`,
`x11-display-served-on-its-vnc-port-is-not-offered`.

Code issues: `tddy-daemon-kernel` `oversized-file-config.md` regressed 1,557 → 1,575 production
lines, and `tddy-host-service` `oversized-file-host-tooling.md` 775 → 806. Both recorded, not split.

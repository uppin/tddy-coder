# 2026-10-20 — an unreadable X11 socket directory lists no X11 channels instead of a failed reading

**Category:** Behaviour gap
**Source:** `#host-desktops` 7/8 (`feature/host-desktops/x11-channels`, PR #659)

- `packages/tddy-host-service/src/x11_displays.rs` `with_x11_channels`: when `list_x11_displays`
  returns an error other than `NotFound` (a socket directory the daemon user may not read, an entry
  that fails mid-listing), the daemon logs `could not list X11 displays in <dir>: <err>` and returns
  the endpoint readings alone. The Hosts screen then shows a host with no X11 channels, which reads
  as "no displays here" when the truth is "the daemon could not look".
- `list_x11_displays` itself does propagate the error (`std::io::Result`); only the aggregation into
  the tooling block swallows it. A missing directory is deliberately empty
  (`a_host_with_no_x11_socket_directory_lists_no_displays`); no test pins the other error path.
- **Why deferred:** the honest fix is the tooling probe's own convention, a `ProbeOutcome::Failed`
  reading, but no channel identity exists for it. `HostRemoteDesktop` names an X11 channel by
  `uint32 display`, and display `0` is a real display, so a failed reading for "the socket
  directory" would claim `:0` was unreadable. Giving it an identity of its own is a `host.proto`
  contract change (a third `DesktopChannelKind`, or an `optional` display) that #659's draft
  contract did not include and the successor #660 is also editing nearby.
- **Fix shape:** a `DESKTOP_CHANNEL_KIND_X11_SOCKET_DIR` (or an `optional uint32 display`) reading
  with `outcome = PROBE_FAILED`, `failure_reason` = the I/O error, `channel_id = "x11"`, label
  "X11 displays". The row's `desktopFact` already renders a non-OK X11 reading as "Could not check
  the display" and offers no Connect for it. Test: a socket directory with mode `000`.

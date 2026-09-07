# 2026-09-06 — A remote-desktop reading on the host tooling probe
**Type:** Feature

`remote_desktop_probe.rs` adds a fourth block to `HostTooling`: one `DesktopReachability` per
protocol — `{ outcome, protocol, can_bridge, desktop_reachable, port }` — for VNC on **5900** and
RDP on **3389**. `host_tooling.rs` fills it beside the git, `gh` and ssh-agent blocks, and
`GetHostToolingResponse.remote_desktop` carries it as a `repeated HostRemoteDesktop`.

**Two facts, never one.** `can_bridge` is an existence check on the resolved bridge binary;
`desktop_reachable` is a bounded TCP connect. They are independent, both are answered on every
reading, and collapsing them would send an operator to install a bridge on a host that has one, or
to start a desktop on a host that is already serving.

**A refusal is a finding; anything else is a failure.** `is_accepting_connections` maps
`ErrorKind::ConnectionRefused` to `Ok(false)` — we reached the host and nothing is listening — and
**every other** error, timeout included, to `Err(reason)`, which becomes `ProbeOutcome::Failed` with
`desktop_reachable: false`. That is the same rule `classify_gh_auth_status` follows: a reader keying
off the flag alone would report "no desktop" for a host nobody reached.

The connect writes **no bytes** and closes immediately — never a protocol handshake, since the
Hosts screen probes every host it lists on every poll — and is bounded at `CONNECT_TIMEOUT`
(750 ms), far inside `PROBE_TIMEOUT`. `writes_no_bytes_to_the_remote_before_closing` has the fake
listener record what it received, because otherwise "we do not handshake" is a comment rather than a
fact. The address is always the daemon's own loopback, so each daemon answers for the machine it
runs on and `GetHostTooling`'s existing peer routing is the whole fan-out.

Three points that are easy to get wrong and were:

- **The bridge check must test the path this daemon would really spawn.** `TcpRemoteDesktopProbe`
  carries a `DaemonConfig` and `SubprocessHostToolingProbe::for_config` builds it from the daemon's
  own configuration, which `connection_service.rs` passes from the live `config` already in scope.
  Answering from `DaemonConfig::default()` would report an operator's explicitly configured,
  installed bridge as absent — the same fabricated fact, pointing the other way. Resolution *order*
  in `config.rs` is untouched; only existence is asked about.
- **A bare `PATH` name is searched on `PATH`, not `exists()`-ed.** The resolvers' last resort is a
  bare `"tddy-vnc"`, which the OS looks up on `PATH`. `Path::exists()` on it would answer about the
  daemon's working directory — `/` under the `--systemd` unit, which sets no `WorkingDirectory=` —
  and report "cannot bridge" for a genuinely installed bridge. `binary_is_present` splits on whether
  the resolution contains a directory.
- **Field order is the deadline.** The readings are collected into a local **before** the
  `HostTooling` literal, beside `ssh_agent`: a struct literal evaluates its fields in order, so
  `remote_desktop` sitting last would have had two bounded connects judged on whatever `git` and
  `gh` left of the shared deadline — and `gh` can reach the network. The block would then report a
  host as unprobeable for time another probe spent.

The `#[cfg(not(unix))]` arm **probes for real** rather than reporting `Unsupported` like the three
blocks above it: nothing here needs `start_output_as_user`, so a loopback connect and a `stat`
answer on every platform, and declaring them unsupported would be as much an invention as a finding
nobody made. The code says so where a reader would assume symmetry.

`protocol` is an `int32` holding `screen_sharing.proto`'s `Protocol` values (1 = VNC, 2 = RDP), which
`DesktopProtocol`'s discriminants mirror rather than restate — two enums meaning the same thing
drift, and this one has to agree with the service that spawns the bridges.

The test seam is on `SubprocessHostToolingProbe` (`probing_desktops_with`) and deliberately **not** a
second builder override on the service: `with_host_tooling` already substitutes the whole
`HostTooling`, `remote_desktop` included, so a parallel `with_remote_desktop_probe` would have had no
caller. Dead surface is worse than a missing one. 20 tests pass under the two filters —
`remote_desktop_probe` (6) and `host_tooling` (14).

**Known cost, recorded not pre-optimised:** probing on every screen refresh is a TCP connect per host
per protocol. If a fleet makes that noticeable, the fix is a short-TTL cache in front of the block.

Node 7 of the `#hosts-screen` stack — [PR #459](https://github.com/uppin/tddy-coder/pull/459).
Starting a stream from a host row is [PR #460](https://github.com/uppin/tddy-coder/pull/460).

See [`host-tooling-probe.md`](../host-tooling-probe.md) and
[`connection-service.md`](../connection-service.md).

# Changeset: livekit-ice-gather-fd-leak

Incident fix: the installed `tddy-daemon` exhausted its 1024 fd soft limit after ~20 h of
uptime and failed every open from then on — `Projects unavailable: read
~/.tddy/projects/projects.yaml: Too many open files (os error 24)` — for over a day before
it was diagnosed.

## Root cause

tddy connects to LiveKit with `RoomOptions::default()`, and both the livekit crate
(0.7.47) and the libwebrtc binding (0.3.37) default `rtc_config.continual_gathering_policy`
to `GatherContinually`. Every network change (DHCP lease renewal on either physical
interface every ~27 min, docker veth create/destroy churn) makes the ICE agent re-gather,
and the superseded host-candidate sockets are leaked — the same failure reported upstream
as [livekit/rust-sdks#1512] ("regather on failed networks leaks the superseded host UDP
sockets (EMFILE after ~30 h on a multi-homed Linux host)"). This host is multi-homed
(two physical NICs plus `docker0`), so each regather round leaks a socket per interface.

Measured on the affected host: ~1000 leaked UDP/TCP-LISTEN candidate sockets spread across
the whole ephemeral range over 46 h of daemon uptime, ~40–60/h while saturated, EMFILE
error spam from Oct 30 08:58 onward.

## Responsibility

- `tddy-livekit::tddy_room_options()` — the SDK default with ICE gathering pinned to
  `GatherOnce`, with a unit test asserting the policy.
- Every tddy-owned LiveKit connect uses it: daemon common-room discovery
  (`livekit_peer_discovery`), common-room roster joiner (`common_room_supervisor`), session
  room join (`session_room`), terminal exposure, screenshare bridge + streamer,
  screen-capture streamer, coder CLI participant host, and `connect_client` (tool RPC
  clients).
- Network changes no longer leak sockets. A network change that breaks an established
  connection is handled the way it already was: the SDK's reconnect flow builds a fresh
  connection that gathers fresh candidates.

## Boundaries

- Does **not** change the livekit crate version — the upstream fix for #1512 is not in a
  release we can jump to cheaply (0.7 → 0.10 is a major-version jump); revisit once the
  upgrade is otherwise due.
- Does **not** change test-only connects: tests keep `RoomOptions::default()`.
- Does **not** address the leaked sockets of the already-running daemon — a restart clears
  them; the unit's default `LimitNOFILE=1024` is a separate operational concern (the
  incident host was mitigated live with `prlimit --nofile=8192`).

## Dependencies

None — leaf bugfix.

## Draft PR contract

1. Failing test first: `room_options::tests::gathers_ice_candidates_once_not_continually`
   asserts `GatherOnce` (red against the SDK default, green after the change).
2. Scoped verification: `./test -p tddy-livekit -p tddy-daemon-livekit -p tddy-screenshare
   -p tddy-livekit-screen-capture -p tddy-coder-cli` and scoped clippy on the same set.
   Integration targets that start a LiveKit testkit container can fail with `failed to bind
   host port … /udp: address already in use` on a host whose daemon still holds ~1000
   leaked UDP ports — a retry of the same target is expected to pass.

[livekit/rust-sdks#1512]: https://github.com/livekit/rust-sdks/issues/1512

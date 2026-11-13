# One libwebrtc runtime for the life of the process

**Type:** Fix
**Packages:** `tddy-livekit`, `tddy-daemon-livekit`

## Problem

On 2026-11-13 the production daemon (`mantas-udoo-ros`, built from `20f475576`, livekit SDK
0.7.47) stopped answering HTTP after forty hours: port 9000 accepted connections and never wrote a
byte. The process sat at exactly its 1,024-descriptor soft limit, and the descriptors were not
sockets:

| Kind | Count |
|---|---|
| pipe ends | 672 |
| `anon_inode:[eventpoll]` | 339 |
| sockets | 9 |
| threads | 1,028 — 337 in `ep_poll`, 681 in `futex_do_wait` |

One libwebrtc runtime is exactly that fingerprint: `webrtc-sys`'s `RtcRuntime` starts a network
thread with a `PhysicalSocketServer` (one epoll instance, one wake-up pipe) and a worker and a
signaling thread (parked on a futex). The daemon held 337 of them.

They accumulate through two facts:

1. **The SDK drops a failed session without closing it.** `EngineInner::connect`'s `try_connect`
   does `RtcSession::connect(..)` and then `session.wait_pc_connection().await?`. When the ICE wait
   times out, the `RtcSession` is dropped — and `RtcSession` has no `Drop`, while the four tasks it
   spawned (`signal_task`, `rtc_session_task`, `data_channel_task`, `dt_sender_task`) each hold an
   `Arc<SessionInner>` and run until a `close()` nobody calls. The session's peer connections, and
   through the native `PeerConnection`'s `shared_ptr<RtcRuntime>` the runtime they were created on,
   live for the rest of the process.
2. **The SDK recreates the runtime whenever nobody holds it.** `LkRuntime::instance()` keeps only a
   `Weak`; the engine holds the `Arc` for the duration of one `Room::connect`. The daemon runs two
   independent join loops against the common room (`common_room_supervisor` and
   `livekit_peer_discovery`), and whenever both are between attempts at the same moment the last
   `Arc` goes, the `PeerConnectionFactory` is destroyed, the leaked sessions keep the old
   `RtcRuntime` alive, and the next join builds a new one — three threads, one epoll, two pipe ends,
   for good.

What made the loops fail at all is a separate, still-open fact about the host: the LiveKit server
runs with `use_external_ip: true`, so it advertises only its public address as a host candidate,
and the daemon on the same machine cannot hairpin to it. Every join since the 2026-11-10 deploy
ended in `wait_pc_connection timed out` (12,751 of them across the three daemon runs; zero
`LiveKit connected`). The same run of the daemon exhausted its descriptors once before, on
2026-11-12, and was restarted. At the current cadence a new runtime leaks every two to three minutes.

## Change

`tddy_livekit::pin_webrtc_runtime()` holds one `Arc<LkRuntime>` in a `OnceLock` for the life of
the process. Every room-connecting entry point in `tddy-livekit` (`LiveKitParticipant::connect`,
its reconnect path, `connect_client`) and the daemon's direct `Room::connect` in
`livekit_peer_discovery` call it first, so `LkRuntime::instance()` always upgrades to the pinned
runtime and the process never builds a second one. The daemon's descriptor and thread counts are
then flat across any number of failed joins.

What this does **not** fix: the SDK still leaks each failed session's `SessionInner`, peer
connections and four parked tasks — memory, not descriptors or threads, since a failed session's
ICE sockets are closed and the server closes its signal websocket. That is the SDK's defect and is
recorded in `../todo/2026-11-13-livekit-failed-join-leaks-its-session.md`; the fix belongs in
`EngineInner::connect` (close the session on a failed `wait_pc_connection`), and is still present
on the SDK's `main` (0.11.0).

## Evidence

`packages/tddy-livekit/tests/webrtc_runtime_bounded.rs` joins the testkit server three times
through `LiveKitParticipant::connect` with relay-only ICE and no TURN server, so each join fails
at `wait_pc_connection timed out` the way the daemon's did, and asserts the process's thread and
pipe-descriptor counts settle back to what they were after the first failure. Against the unpinned
code the two extra joins left **+4 pipe descriptors and +4 threads** that never came back
(`ProcessResources { threads: 26, pipe_descriptors: 8 }` against a baseline of `22 / 4`); pinned,
the test passes in about 50 seconds.

## The descriptor-limit discrepancy

`systemctl show` reported `LimitNOFILE=65536` while the failing daemon ran with a soft limit of
1,024 and a hard limit of 524,288 — systemd's `DefaultLimitNOFILE` pair on that host. The journal
settles it: the supervisor that spawned the failing daemon started at 2026-11-12 05:56:32 EET; the
drop-in `/etc/systemd/system/tddy-supervisor.service.d/limits.conf` was created, and
`daemon-reload` ran, at 05:56:58 — twenty-six seconds later. `systemctl show` describes the loaded
unit, not the running process. Nothing in this repository touches `RLIMIT_NOFILE`, and the
supervisor passes its own limits to the daemon unchanged. The 2026-11-13 restart was the first
start under the drop-in, which is why both processes now show 65,536.

## Documentation delta

- `packages/tddy-livekit/docs/` has no page on the runtime; the wrap adds a short section — the
  pinned runtime, why, and the SDK defect it works around — alongside the `tddy_room_options`
  gather-once note, since both are "the SDK leaks descriptors unless told otherwise" rules.
- `packages/tddy-daemon-livekit/docs/` should note that the discovery connect pins the runtime
  before `Room::connect`, and that a new direct `Room::connect` anywhere in the daemon must do the
  same (or go through `tddy-livekit`).

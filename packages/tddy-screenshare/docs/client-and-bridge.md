# Screen-sharing client and bridge (tddy-screenshare)

`tddy-screenshare` is the protocol-agnostic half of every screen-sharing bridge binary. A protocol
crate (`tddy-vnc`, `tddy-rdp`, `tddy-ios-emulator`) implements `ScreenSharingClient`; its `main`
reads a JSON config from stdin and calls `run_bridge::<C>` (or `run_inspect::<C>` under
`--inspect`, chosen by `BridgeMode`).

## `ScreenSharingClient`

| Member | Meaning |
|---|---|
| `connect(&ConnectTarget, &Credentials)` | open the desktop the target names |
| `inspect(&ConnectTarget)` | the sign-in methods it advertises, read without signing in |
| `framebuffer_dimensions`, `get_rgba_frame` | the current frame, RGBA |
| `poll_events` | advance the connection; `true` when the frame changed |
| `request_frame_update(incremental)` | ask a push protocol for a frame (a no-op for pull models) |
| `inject_pointer(x, y, button_mask)` | RFB button mask: bit 0 left, 1 middle, 2 right |
| `inject_key(keysym, pressed)` | X11 keysym |
| `inject_wheel(x, y, delta_x, delta_y)` | a scroll at framebuffer point `(x, y)`, deltas in framebuffer pixels, positive down and right |
| `stop` | close |

**`ConnectTarget { host, port, device }`.** `device` names one device on the host — an iOS
Simulator's udid, with `port` 0 — and is `None` for network desktops;
`ConnectTarget::network(host, port)` builds one of those. `BridgeConfig.device`
(`#[serde(default)]`) carries it from the daemon, and `BridgeConfig::connect_target()` assembles the
target. `run_inspect` connects with `ConnectTarget::network` from an `InspectTarget { host, port }`,
which carries no device: a simulator advertises no sign-in.

**Wheel, per protocol.**

| Client | A scroll becomes |
|---|---|
| `tddy-vnc` | one press-and-release of RFB buttons 4 (up), 5 (down), 6 (left), 7 (right) per axis that moved, at the pointer (`wheel_button_masks`) |
| `tddy-rdp` | one fast-path mouse PDU per axis: `VERTICAL_WHEEL` with −120 × sign(dy) (RDP's vertical wheel is positive away from the user), `HORIZONTAL_WHEEL` with 120 × sign(dx) (`wheel_mouse_pdus`) |
| `tddy-ios-emulator` | a short swipe against the scroll |

Only the sign of each delta is used by VNC and RDP: one notch per event, however far it scrolled.

## `run_bridge`

Connects, publishes a LiveKit video track, reports its start on stdout
([bridge readiness](../../tddy-daemon-extensions/docs/bridge-readiness.md)), and serves
`ScreenSharingInputService` over the data channel. `stream_input` turns each
`ScreenSharingInputEvent` — `pointer`, `key` or `wheel` (`ScreenSharingWheelEvent { x, y, delta_x,
delta_y }`, `sint32` deltas) — into an `InputCmd`; the pump loop drains the whole backlog before
sampling each frame and calls the matching `inject_*`. An event with no case set is skipped.

## Frames to the encoder

`streamer.rs` hands frames to `NativeVideoSource::capture_frame` as I420, converted by
`rgba_to_i420`. libyuv names a format by its 32-bit word read little-endian, so its `ABGR` is the
byte order R, G, B, A — RGBA in memory — and the frame is passed to `abgr_to_i420` as it is.
Reordering the bytes first reads alpha as red and washes every colour towards pink;
`tests/rgba_to_i420.rs` pins the conversion of known colours.

A frame of a size other than the one the stream started at is refused, so a desktop that changes
resolution mid-stream stops updating.

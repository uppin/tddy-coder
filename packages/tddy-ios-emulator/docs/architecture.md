# tddy-ios-emulator — architecture

`IosSimulatorClient` is a `ScreenSharingClient` composed of three ports. Every rule about which
device, which pixels and which point is pure code over those ports. The macOS adapters only read the
system.

```
viewer ──input (data channel)──▶ run_bridge ─▶ IosSimulatorClient ──InputSink──▶ dtuhidd (in the device)
viewer ◀──screenshare track───── run_bridge ◀─ IosSimulatorClient ◀─SimulatorWindows── device window (xcap)
                                               IosSimulatorClient ──Simctl──▶ xcrun simctl list devices --json
agent shell ──xcrun simctl install|launch <udid>────────────────────────▶ CoreSimulator
```

## Modules

| Module | Owns |
|---|---|
| `simctl` | `Simctl` port, `SimDevice { udid, name, runtime, state }`, `DeviceState` (`Booted`, `Shutdown`, anything else kept verbatim), `parse_device_list` (drops devices whose runtime is unavailable; `com.apple.CoreSimulator.SimRuntime.iOS-18-2` → `iOS 18.2`), and the `XcrunSimctl` adapter |
| `window` | `SimulatorWindows` port (`find(device_name)`, `capture(window)`), `SimulatorWindow` (id, owner pid, title, `bounds`, `device_area`, `scale`), `Frame` (RGBA) |
| `geometry` | pure: `Rect`, `ScreenPoint`, `DeviceViewport` (the crop of a captured window to the device area, the streamed frame size `device_area × scale`, and device pixel → screen point), `below_title_bar` |
| `input` | `InputSink` port, `InputEvent` (`MouseMove`/`Down`/`Drag`/`Up`, `Text`, `Key`), `SpecialKey`, `key_for_keysym` (X11 keysym → `KeyStroke`) |
| `client` | `IosSimulatorClient`, `SimulatorPorts`, `swipe_path`; the production `connect` (macOS, and a refusing one elsewhere) |
| `dtuhid` | pure DTUHID wire model: the XPC dictionaries `dtuhidd` takes, as `WireValue`s, testable without XPC |
| `dtuhid_xpc` (macOS) | `DtuhidInput`, the production `InputSink`: sends those messages to the device's `dtuhidd` over XPC |
| `macos` (macOS) | `XcapSimulatorWindows`, the production `SimulatorWindows`; `require_permissions` |
| `listing` | `--list-devices`: `booted_devices_json`, `list_booted_devices`, `LIST_DEVICES_FLAG` |
| `error` | `SimulatorError`: `NoDevice`, `DeviceNotFound`, `DeviceNotBooted { state }`, `WindowNotFound`, `Unsupported` — each names the udid |

## Connect

`connect(target, _)` reads the udid from `ConnectTarget.device` (`NoDevice` when absent), then:

1. **The device.** `simctl list devices --json` must list the udid (`DeviceNotFound`) in state
   `Booted` (`DeviceNotBooted`, naming the state). This runs before the permission check, so an
   unknown udid is reported as such whatever is granted.
2. **Permissions.** `CGPreflightScreenCaptureAccess` (Screen Recording: without it a capture is
   blank) and `AXIsProcessTrusted` (Accessibility: without it Device Hub's device area cannot be
   found). Each failure names the System Settings pane. There is no fallback.
3. **The window.** `find(device.name)` (`WindowNotFound` when none).
4. **Input.** `DtuhidInput::connect(udid, window.device_area)`.

The framebuffer is the device area at the display's backing scale, not the window. The bridge
reports `Ready { width, height }` with that size, or `Failed { kind: Connect }` carrying the
`SimulatorError` text.

## Finding the window and cropping it

`XcapSimulatorWindows::find` walks `xcap::Window::all()` for windows owned by **`Simulator`**
(Xcode ≤ 26) or **`Device Hub`** (Xcode 27, where DeviceHub.app replaced Simulator.app). A window
shows the device when its title is the device name alone or `<name> – <runtime>` (or `<name> -`),
so "iPhone 16" never takes an "iPhone 16 Pro" window.

The device area depends on the app:

| Owner | Device area |
|---|---|
| `Simulator` | the window below its 28-point title bar (`below_title_bar`) |
| `Device Hub` | the frame of the accessibility element with subrole **`iOSContentGroup`**, found in the owner's AX tree: the AX window whose frame matches the window-server frame within 1 point, then a breadth-limited search (depth ≤ 12, ≤ 4 000 elements visited, 1 s messaging timeout per call). None found fails the connect, naming the window |

The window bounds and device area are read **once, at connect** (`TODO(ios-simulator)` in
`macos.rs`): a window moved or resized afterwards is not followed.

## Frames

The model is a pull. `poll_events` captures the whole window (`xcap` `capture_image`), crops it to
the device area through `DeviceViewport` (a capture a pixel off the expected size is cropped or
padded to it) and returns `true` only when the frame differs from the previous one; the first
capture is always a change. `request_frame_update` does nothing.

xcap copies the window image in the display's colour space (Display P3 on Apple displays) with no
conversion, and at its displayed size, so the stream is slightly desaturated and only as sharp as
the window's zoom.

## Input — `dtuhidd`

Device pixels from the viewer are mapped to screen points inside `device_area`
(`DeviceViewport::to_screen`), then `dtuhid::normalised` turns them into the `0..=1` position on the
device screen that `dtuhidd` takes. Input therefore reaches the device whatever covers or moves its
window.

| Viewer input | Client | `dtuhidd` |
|---|---|---|
| left button down / held move / up (RFB mask bit 1) | `MouseDown` / `MouseDrag` / `MouseUp` | one digitizer contact: the first down starts it, later downs move it, the up ends it (idb's `DigitizerContactTracker`) |
| move with no button | `MouseMove` | nothing |
| wheel / trackpad `(dx, dy)` | `swipe_path`: a touch-down at the pointer, 4 moves, and a lift at `start − delta`, clamped to the screen; ignored while a touch is down | a short swipe against the scroll, as a thumb moves content |
| printable keysym `0x20..=0x7e` on press | `Text` | the US-layout HID usage pressed and released, inside a held left shift when the character needs one; a character with no usage is dropped |
| Return, Backspace, Escape, Tab, arrows | `Key { pressed }` | the key's HID usage |
| any other keysym | — | nothing |

**Why `dtuhidd`.** From Xcode 27 the simulator drops Indigo keyboard events and delivers Indigo
touch unreliably, and Device Hub ignores CGEvents posted to its process. `dtuhidd` inside the guest
takes touch, keyboard and hardware-button events as XPC dictionaries on its digitizer service. The
connection, ported from idb:

- CoreSimulator is a private framework, loaded at run time from the active Xcode
  (`xcode-select -p`) and messaged through the Objective-C runtime; nothing links against it.
  `-[SimDevice lookup:error:]` finds the service's Mach port in the device's bootstrap namespace.
- The port is wrapped with the private `xpc_endpoint_create_mach_port_4sim` and the connection
  marked `xpc_connection_enable_sim2host_4sim`.
- **A liveness barrier** — an inert keyboard message with usage 0 — is round-tripped before any
  event, because a send to a throttled daemon reports no error. It is retried while launchd
  throttles a `dtuhidd` that aborted during boot, and the probe thread holds its own retained
  reference to the connection so a timed-out attempt cannot release it under a blocked send.

Pinch and other multi-touch are not forwarded: the viewer sends one pointer, and the wire model's
second contact point is unused.

## Failure modes

| Situation | Start reports |
|---|---|
| target names no device | `Failed{Connect}` "an iOS Simulator target must name a device udid" (the daemon refuses such a target at `AddHostTarget` already) |
| udid unknown | `Failed{Connect}` "no iOS Simulator with udid …" |
| device not booted | `Failed{Connect}` "iOS Simulator … is Shutdown, not booted" |
| Screen Recording / Accessibility missing | `Failed{Connect}` naming the permission and its settings pane |
| no window titled for the device (closed) | `Failed{Connect}` "Simulator.app shows no window for …" |
| Device Hub window with no `iOSContentGroup` | `Failed{Connect}` "Device Hub shows no iOS screen for …" |
| `dtuhidd` never answers the barrier | `Failed{Connect}` |
| not macOS | `Failed{Connect}` "… can only be bridged on macOS" |

A headless or locked-screen Mac cannot stream: capture needs the window on the console.

## Attribution

`dtuhid.rs` follows idb's `FBSimulatorControl/HID/DTUHIDModels.swift`,
`SimulatorDTUHIDConnection.swift`, `SimulatorDigitizerHIDTransport.swift`,
`SimulatorHIDButtonIdentity.swift` and `SimulatorIndigoHID.swift`, with XPC value types from
`FBSimulatorControl/XPC/XPCEncoder.swift`; `dtuhid_xpc.rs` follows
`SimulatorXPCConnection.swift` and `SimulatorDTUHIDConnection.swift`. idb is MIT-licensed,
Copyright (c) Meta Platforms, Inc. and affiliates; the licence text is in
[`THIRD_PARTY_NOTICES`](../THIRD_PARTY_NOTICES).

## Tests

| Suite | Covers |
|---|---|
| `tests/simctl_parse.rs` | `simctl` JSON → `SimDevice`, unavailable runtimes dropped |
| `tests/geometry.rs`, `src/geometry.rs` tests | crop and device → screen mapping at 2× |
| `tests/keysyms.rs` | keysym table |
| `tests/dtuhid_wire.rs` | exact DTUHID dictionaries, contact phases, normalisation |
| `tests/client_acceptance.rs` | the client over in-memory ports: connect failures, device-sized framebuffer, drag → down/drag/up, keys, change-only frames, wheel → swipe |
| `tests/device_listing.rs` | `--list-devices` JSON |
| `tests/bridge_binary.rs` | the real binary reports `Failed{Connect}` with the udid for an unknown device |
| `tests/simulator_production.rs` | `#[ignore]`: a real booted simulator streams and a tap reaches it |

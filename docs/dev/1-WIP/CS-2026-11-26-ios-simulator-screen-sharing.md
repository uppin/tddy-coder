# Changeset: iOS Simulator as a screen-sharing target

**Created:** 2026-11-26
**Status:** Created
**PRD:** [PRD-2026-11-26-ios-simulator-screen-sharing.md](../../ft/web/1-WIP/PRD-2026-11-26-ios-simulator-screen-sharing.md)

- [x] Create/update PRD documentation
- [x] Create changeset

## Affected Packages

- [ ] `tddy-ios-emulator` (**new**): `IosSimulatorClient: ScreenSharingClient`, the `simctl`
  device layer, window capture, CGEvent input, and the bridge binary
- [ ] `tddy-screenshare`: `connect` takes a `ConnectTarget` that can name a device; `BridgeConfig` gains `device`
- [ ] `tddy-vnc`, `tddy-rdp`: adopt the new `connect` signature (mechanical)
- [ ] `tddy-service`: `screen_sharing.proto` gains `PROTOCOL_IOS_SIMULATOR` and `ScreenSharingTarget.device_id`
- [ ] `tddy-daemon-kernel`: `resolve_ios_simulator_binary_path` and `screen_sharing.ios_simulator_binary_path`
- [ ] `tddy-screen-sharing`: `prepare_bridge` routes the new protocol and carries `device_id` into the spawn config
- [ ] `tddy-host-service`: new `ios_simulators` module offers each booted simulator as a desktop **channel** in the host tooling probe (beside X11 displays); `HostDesktopTarget.device`
- [ ] `tddy-client-web` / `tddy-hosts-web`: regenerated protos; the iOS label and icon on a host target
- [x] Release and install scripts: `./release` builds `tddy-ios-emulator`, `./install` (`INSTALLED_BINARIES`) and `./install --desktop` (`DESKTOP_BINARIES`) ship it beside the daemon on every platform (developer, 2026-11-26). `scripts/dev-runtime-binaries.sh` and `publish.sh` unchanged

## Related Documentation

- [screen-sharing-service.md](../../../packages/tddy-screen-sharing/docs/screen-sharing-service.md)
- [docs/ft/web/screen-sharing-sessions.md](../../ft/web/screen-sharing-sessions.md)
- [tddy-livekit-screen-capture README](../../../packages/tddy-livekit-screen-capture/README.md) (xcap window capture, macOS permission check)
- [tddy-web remote-desktop-input.md](../../../packages/tddy-web/docs/remote-desktop-input.md) (the browser input client this reuses unchanged)

## State A (Current)

- `tddy_screenshare::ScreenSharingClient` (`client.rs`) is the protocol-agnostic bridge trait:
  `connect(host, port, &Credentials)`, `inspect(host, port)`, framebuffer RGBA,
  `poll_events`, `request_frame_update`, `inject_pointer(x, y, button_mask)` (RFB mask),
  `inject_key(x11_keysym, pressed)`, `stop`. `run_bridge::<C>` streams it to LiveKit and serves
  `ScreenSharingInputService` over the data channel. `BridgeMode` dispatches `--inspect`.
- Implementations: `tddy-vnc` (`VncClientState`) and `tddy-rdp` (`RdpClient`). Each binary is about 20 lines:
  read a `BridgeConfig` JSON from stdin, then call `run_bridge`.
- `screen_sharing.proto` `Protocol { UNSPECIFIED, VNC, RDP }`. A target is network-addressed
  (`host`, `port`), plus an optional `x11_display` for a daemon-started `x11vnc`.
- `tddy-screen-sharing::bridge_config::prepare_bridge` maps the protocol to a binary through
  `tddy-daemon-kernel` `resolve_{vnc,rdp}_binary_path` (config override, else a sibling of the daemon).
  **The bridge always runs on the daemon's machine** and dials `host:port`.
- `tddy-livekit-screen-capture` already captures a macOS **window** with xcap 0.9, checks the
  Screen Recording permission (`macos_access.rs`), and depends on `objc2-core-graphics 0.3.2`
  (`CGWindow`). It is a standalone CLI with no input path and no `ScreenSharingClient` impl.
- Agents deploy nothing to devices. No package mentions `simctl` or the iOS Simulator.
- `tddy-sandbox-darwin` renders `mach-lookup` as `All` or an allow-list of global names. Whether a
  jailed agent can reach `com.apple.CoreSimulator.CoreSimulatorService` is **unverified**.

## Prerequisites (deferred-work cross-check)

| Record | Class | Note |
|---|---|---|
| `docs/dev/todo/2026-10-20-screen-sharing-two-stand-in-bridges.md` | ⚠ during | The new protocol's service tests reuse `tests/common::StandInBridge`. Do not add a third stand-in. |
| `docs/dev/todo/2026-10-18-screen-share-mid-session-resize.md` | ℹ answered | A rotated simulator changes size mid-stream, so this behaves like that open item. Rotation stays out of scope here and is recorded as debt. |
| `docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md` | ℹ answered | The browser input client exists, so iOS input needs no web work. The wheel is still open, so there is no iOS scroll by wheel. Swipe works by drag. |
| `packages/tddy-screen-sharing/docs/code-issues/`, `packages/tddy-screenshare/docs/code-issues/` | none | Neither directory exists, so no claimed issues. |

## State B (Target)

An operator on a macOS host opens **Hosts → this Mac**, sees each **booted iOS Simulator** listed
as a desktop target (for example "iPhone 16 Pro, iOS 18.2"), and starts it. The daemon spawns
`tddy-ios-emulator`, which streams the simulator's **device screen** (the Simulator window cropped
to the device) as a LiveKit screenshare. Clicks and drags in the browser become taps and swipes,
and key presses become typed text. An agent session on the same Mac deploys with plain
`xcrun simctl install|launch|openurl <udid> …`, and the change shows up in the shared screen.

```
browser ──input (data channel)──▶ tddy-ios-emulator ──CGEvent──▶ Simulator.app window (device udid)
browser ◀──screenshare track──── tddy-ios-emulator ◀──xcap───── Simulator.app window
agent shell ──xcrun simctl install/launch <udid>──────────────▶ CoreSimulator
```

### `tddy-ios-emulator` (new crate, macOS only; on other platforms it compiles to a binary that fails `connect`)

| Module | Owns |
|---|---|
| `simctl` | `Simctl` port (trait) + `XcrunSimctl` adapter: `list_devices() -> Vec<SimDevice>` (parses `xcrun simctl list devices --json`), `boot(udid)`, `install(udid, app)`, `launch(udid, bundle_id)`. `SimDevice { udid, name, runtime, state }` |
| `window` | `SimulatorWindows` port + `XcapSimulatorWindows`: find the Simulator.app window whose title names the device, its bounds, and an RGBA capture |
| `geometry` | Pure. `DeviceViewport`: the window→device crop rectangle (strip title bar and bezel) and the device→screen point mapping used by input |
| `input` | `InputSink` port + `CgEventInput`: posts mouse down/drag/up and unicode key events at **screen** coordinates; X11 keysym → `CGKeyCode` / unicode table |
| `client` | `IosSimulatorClient: ScreenSharingClient`, which composes the three ports. `connect` resolves `device` (the udid) to a **booted** device and its window, and fails with a typed reason otherwise. `poll_events` captures one frame and returns `true` when it differs from the last one. `request_frame_update` is a no-op (a pull model, like RDP's push no-op). `inspect` returns no sign-in methods |
| `permissions` | Screen Recording + Accessibility (`AXIsProcessTrusted`) preflight. A missing permission reports `Failed { kind: Connect }` with a message naming the permission |
| `main.rs` | The same 20-line shape as `tddy-vnc`: `BridgeMode` → `run_bridge::<IosSimulatorClient>` / `run_inspect` |

### `tddy-screenshare`

- `ScreenSharingClient::connect(target: &ConnectTarget, credentials: &Credentials)` where
  `ConnectTarget { host, port, device: Option<String> }` (`ConnectTarget::network(host, port)` for VNC/RDP). `inspect` takes the same `ConnectTarget`.
- `BridgeConfig.device: Option<String>` (`#[serde(default)]`) and `BridgeConfig::connect_target()`. `InspectTarget` (in `tddy-daemon-extensions`) is **unchanged**: a simulator advertises no sign-in, so inspect needs no device.

### Proto and daemon

- `screen_sharing.proto`: `PROTOCOL_IOS_SIMULATOR = 3`, `ScreenSharingTarget.device_id = 9`,
  `AddHostTargetRequest.device_id = 10`. `host.proto`: `DESKTOP_CHANNEL_KIND_IOS_SIMULATOR = 3`,
  `HostRemoteDesktop.device_id = 11`.
- `AddHostTarget` for this protocol is refused with `INVALID_ARGUMENT` when `host` is not loopback
  ("an iOS Simulator is bridged only on the Mac that runs it, not at <host>") or `device_id` is empty
  ("an iOS Simulator target must name a device udid"). It is refused when the target is **added**, not when it is
  started, because a remote Mac is reached by running a daemon there.
- `tddy-daemon-kernel`: `resolve_ios_simulator_binary_path` (config `ios_simulator_binary_path`, otherwise
  the `tddy-ios-emulator` sibling binary).
- `tddy-host-service::ios_simulators`: `SimulatorLister` port (`booted() -> Result<Vec<BootedSimulator>, String>`)
  and `ios_simulator_channels(lister, can_bridge) -> Vec<HostRemoteDesktop>`: channel id `ios:<udid>`,
  label `"<name> · <runtime>"`, port 0, kind `IOS_SIMULATOR`. A lister error lists none. The production
  lister runs **`tddy-ios-emulator --list-devices`** (JSON on stdout) rather than linking the crate,
  so host-service takes no dependency on the bridge's LiveKit stack. This mirrors how `--inspect` is
  run. `SubprocessHostToolingProbe` appends these channels next to the X11 ones.
- How simulators are discovered: booted simulators are **channels** in the tooling probe, like X11
  displays (`x11_displays::with_x11_channels`). Connecting one attaches a `HostDesktopTarget`
  with `protocol = 3` and `device`. The plan's first draft had `host_desktop_targets` listing them,
  which was wrong: that module stores attached targets and discovers nothing.

## Delta

### New
- `packages/tddy-ios-emulator` (crate, binary `tddy-ios-emulator`, README, `docs/architecture.md`)
- Proto: `PROTOCOL_IOS_SIMULATOR`, `ScreenSharingTarget.device_id`
- Kernel: `resolve_ios_simulator_binary_path`, config key `screen_sharing.ios_simulator_binary_path`

### Modified
- `tddy-screenshare`: `connect`/`inspect` signatures, plus `BridgeConfig.device` and `InspectTarget.device`
- `tddy-vnc`, `tddy-rdp`: signature adoption only
- `tddy-screen-sharing::bridge_config`: protocol routing, `device_id` propagation, local-host guard
- `tddy-host-service`: `ios_simulators` channels wired into the tooling probe; `HostDesktopTarget.device`
- `tddy-ios-emulator`: a `--list-devices` mode (JSON of booted devices) for the lister
- `tddy-hosts-web`: iOS target label and icon (the existing start/stop and overlay are reused)
- `./release`, `./install`, `./install --desktop` (plus their e2e fixtures and the VM testkit's `deployed_binaries`)

### Removed
- Nothing

## Milestones

### M1: the trait carries a device
- [x] `ConnectTarget`, `BridgeConfig.device` exist; VNC and RDP adopted (compile-checked with `--all-targets`; suites still to run)

### M2: `tddy-ios-emulator` core (pure + ports)
- [x] `simctl list --json` parsing into `SimDevice` (booted, shutdown, unavailable runtimes)
- [x] `DeviceViewport` crop and point mapping
- [x] keysym → key event table (ASCII, Return, Backspace, arrows, Escape)
- [x] `IosSimulatorClient` over in-memory ports: connect, frames, pointer and keys reach the sink at mapped coordinates, stop

### M3: real adapters + binary
- [x] `XcrunSimctl`, `XcapSimulatorWindows`, `CgEventInput`, permission preflight
- [x] `tddy-ios-emulator` binary reports `Ready {w,h}` / `Failed {Connect}` through `BridgeReadiness`

### M4: the daemon offers and starts it
- [x] Proto + kernel resolver + `prepare_bridge` routing + local-host guard
- [x] Host targets list booted simulators on a local Mac
- [x] Release and install scripts ship the binary; web shows the target
  - [x] web lists a booted simulator by device and runtime, and its Connect attaches it (`deviceId`, `127.0.0.1`, port 0) and starts it without inspecting it or asking for a sign-in; the daemon starts an iOS target with `DesktopAuthMethod::None` and raises no prompt (`a_simulator_starts_without_asking_the_operator_to_sign_in`)
  - [x] Tddy Desktop's own host (IPC, `{"rpc"}`) is connectable when the common room is joined and its daemon is on it: `ConnectionProviderRegistry` lends a later wire's `media` to the winning connection (calls and presence stay on IPC); with no LiveKit nothing is lent

### M5: deploy path documented and verified
- [ ] `docs/ft/web/screen-sharing-sessions.md` gains the iOS Simulator flow (deploy with `xcrun simctl install|launch <udid>`)

## Testing Strategy

### Test Level Decisions
| Aspect | Level | Rationale |
|---|---|---|
| `simctl` JSON parsing, viewport math, keysym table | Unit | Pure and deterministic. Fixtures are captured `simctl list --json` outputs |
| `IosSimulatorClient` behaviour | Integration (in-memory ports) | The client is plumbing over three ports, so in-memory adapters assert the exact events and frames with no Mac permissions involved |
| Bridge binary readiness | Integration | Spawn the binary with a `BridgeConfig` naming an unknown udid and assert a `Failed{Connect}` report. This runs on any OS |
| Daemon routing and discovery | Integration | `tddy-screen-sharing` with `StandInBridge`, and `tddy-host-service` with a stub `Simctl` |
| Real Simulator capture + CGEvent | Production test, `#[ignore]` | Needs Xcode, a booted device and TCC grants, so it can't run on CI. It runs by hand on a Mac |

### Acceptance Tests

#### tddy-ios-emulator
- [x] **Unit**: `simctl_devices_from_json_lists_booted_iphone_with_its_runtime` asserts the exact udid, name, `iOS 18.2` and `Booted` from a fixture, and that unavailable runtimes are dropped (`tests/simctl_parse.rs`)
- [x] **Unit**: `viewport_maps_device_corners_to_window_screen_points` checks that the device points (0,0) and (w-1,h-1) map to the exact screen points for a fixture window rect at 2× scale (`src/geometry.rs` tests)
- [x] **Integration**: `connect_to_shutdown_device_fails_naming_the_udid_and_state` (`tests/client_acceptance.rs`)
- [x] **Integration**: `connect_to_booted_device_reports_device_sized_framebuffer`: dimensions equal the cropped viewport, not the window
- [x] **Integration**: `left_drag_becomes_down_drag_up_at_mapped_points` checks that the sink records exactly `[Down(p1), Drag(p2), Up(p2)]`
- [x] **Integration**: `keysym_a_and_return_type_text_and_newline`
- [x] **Integration**: `poll_events_reports_change_only_when_frame_differs`
- [x] **Integration**: `bridge_binary_reports_connect_failure_for_unknown_device` checks that the readiness line is `Failed{Connect}` with the udid in its reason (`tests/bridge_binary.rs`)
- [ ] **Production** (`#[ignore]`, compiles; not run — needs a booted Simulator): `booted_simulator_streams_and_a_tap_reaches_it` (`tests/simulator_production.rs`)

#### tddy-screen-sharing (`tests/ios_simulator_targets_acceptance.rs`; the stand-in is installed as the iOS bridge only)
- [x] `a_simulator_starts_through_the_ios_bridge_at_the_size_it_reported`
- [x] `the_ios_bridge_is_told_which_simulator_to_stream`: stdin JSON `device` is the udid, `port` is 0
- [x] `a_simulator_addressed_at_another_machine_is_refused`: `INVALID_ARGUMENT`, exact message
- [x] `a_simulator_target_without_a_device_is_refused`: `INVALID_ARGUMENT`, exact message
- [x] `an_attached_simulator_is_listed_with_its_device`

#### tddy-host-service (`tests/ios_simulator_channels.rs`)
- [x] `each_booted_simulator_is_offered_as_a_channel_named_by_device_and_runtime`: exact `HostRemoteDesktop`s
- [x] `a_simulator_is_offered_but_marked_unbridgeable_when_the_bridge_binary_is_missing`
- [x] `a_host_that_cannot_list_simulators_offers_none`

#### tddy-daemon-kernel (`tests/ios_simulator_binary_resolution.rs`)
- [x] `resolves_the_ios_bridge_to_the_configured_path`
- [x] `resolves_an_unconfigured_ios_bridge_to_a_sibling_of_the_running_executable`

#### Not covered by a test yet (green writes these)
- [x] `tddy-ios-emulator --list-devices` output, and the production `SimulatorLister` that reads it
- [x] `SubprocessHostToolingProbe` appending simulator channels

#### tddy-vnc / tddy-rdp
- [ ] Existing suites pass unchanged after the signature change (regression gate)

## Technical Debt
- Input is sent to the simulator's `dtuhidd` (DTUHID transport, ported from idb, MIT — `packages/tddy-ios-emulator/THIRD_PARTY_NOTICES`); Xcode 27 drops Indigo keyboard events and delivers Indigo touch unreliably. Pinch / two-finger is not forwarded (the viewer sends one pointer); `pointTwo` is in the wire model but unused.
- A wheel/trackpad scroll on an iOS Simulator is a short swipe per frame, so it has no momentum.
- Colour space: xcap copies the window image's bytes in the display's colour space (Display P3 on Apple displays) with no conversion, so the stream is desaturated. Converting to sRGB needs either `CGWindowListCreateImage` (deprecated, needs an `#[allow(deprecated)]`) drawn into an sRGB bitmap context, or ScreenCaptureKit (`objc2-screen-capture-kit`, a new dependency; also gives native-resolution capture). Needs a developer decision.
- Sharpness: the window is captured at its displayed size (Device Hub's default zoom shows a 1179×2556 device at 594×1292 px). "Physical/pixel-accurate" zoom in Device Hub gives native pixels; capturing above the displayed size needs ScreenCaptureKit or SimulatorKit.
- Rotation or a device switch changes the frame size mid-stream. This is the same open item as `2026-10-18-screen-share-mid-session-resize`.
- Window capture needs the Simulator window **visible and unminimised** on the logged-in console. A headless or locked-screen Mac cannot stream. `idb` (`idb_companion` video-stream + HID) is the upgrade that removes this, and it brings an external dependency.
- No scroll-wheel input (blocked on the browser wheel item).
- Multi-touch gestures (pinch) aren't supported. Simulator's option-drag could cover them later.
- An optional `tddy-tools emulator {list,boot,install,launch}` wrapper for agents. Plain `xcrun simctl` is enough for now.

## Decisions & Trade-offs
- **One crate per OS** (developer, 2026-11-26): `tddy-ios-emulator` now, `tddy-android-emulator` later behind the same trait.
- **Window capture + CGEvent, not `idb` or `simctl io screenshot`**: no new external tools. `simctl io` costs about 200 ms per frame, too slow to use as a stream. Cost: the window must be on screen (see debt).
- **`connect` gains a `ConnectTarget`** rather than putting the udid into `host`: a typed field instead of an overloaded string. Cost: a mechanical edit in `tddy-vnc` and `tddy-rdp`.
- **Local host only**: the bridge runs where the daemon runs, so a remote Mac runs its own daemon. No new transport is needed.
- **No fallback** when a permission is missing. Start fails and names Screen Recording or Accessibility.
- **Dependencies (approved by the developer, 2026-11-26)**: reuse xcap 0.9 and `objc2-core-graphics` (already in the workspace), enable its `CGEvent` / `CGEventSource` features, and add `objc2-application-services` for `AXIsProcessTrusted`.
- **Sandbox (out of scope, developer, 2026-11-26)**: whether a jailed darwin agent can reach CoreSimulator is not investigated in this changeset. Deploys are documented for an agent shell that can run `xcrun simctl`. Any sandbox change is a separate decision.

## Refactoring Needed
### From /ft-dev (acceptance test creation)
### From /red (TDD red phase)
- [x] `TODO(ios-simulator)` markers in `tddy-screen-sharing` `host_targets.rs` (device not yet carried) and `bridge_config.rs` (device not yet listed) are placeholders to replace
- [x] `tddy-client-web` / `tddy-hosts-web` generated TS protos not regenerated yet
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

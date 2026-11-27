# 2026-11-27 — iOS Simulator as a screen-sharing target

**Type:** Feature
**PR:** #1060
**Packages:** `tddy-ios-emulator` (new), `tddy-screenshare`, `tddy-vnc`, `tddy-rdp`, `tddy-service`,
`tddy-daemon-kernel`, `tddy-screen-sharing`, `tddy-host-service`, `tddy-client-web`,
`tddy-livekit-web`, `tddy-hosts-web`, `tddy-web`, `tddy-e2e` / `tddy-vm-testkit` (shipped-set
fixtures); `./release`, `./install`, `scripts/dev-runtime-binaries.sh`

## What landed

- **Bridge.** `tddy-ios-emulator`: a booted simulator's device screen captured with xcap and cropped
  (Simulator.app: below the title bar; Device Hub, Xcode 27: the `iOSContentGroup` AX element);
  input to the device's `dtuhidd` over XPC, ported from meta/idb (MIT). CGEvent posting was tried
  first and Device Hub ignores it; Indigo HID is dropped from Xcode 27; `simctl io screenshot` costs
  about 200 ms per frame. `--list-devices` lists the booted devices for the host probe.
  See [architecture](../../../packages/tddy-ios-emulator/docs/architecture.md).
- **Trait.** `ScreenSharingClient::connect/inspect(&ConnectTarget)` with an optional `device`, plus
  `inject_wheel`; VNC (buttons 4–7) and RDP (wheel PDUs) implement it. RGBA→I420 no longer reorders
  bytes before libyuv (colours were washed pink on every protocol).
- **Daemon.** `PROTOCOL_IOS_SIMULATOR`, loopback-only targets carrying `device_id`, no sign-in;
  `resolve_ios_simulator_binary_path`; booted simulators as `IOS_SIMULATOR` channels in the tooling
  probe.
- **Web.** iOS channels and Connect on the Hosts screen; the overlay fits the picture to the
  desktop's aspect and reads out bitrate / size / fps; wheel and touch forwarding for every desktop;
  the connection registry lends `media` to a winning connection that lacks it, so Tddy Desktop's own
  IPC host can watch a simulator through the common room.
- **Shipping.** `./release`, `./install` (`INSTALLED_BINARIES`), `./install --desktop`
  (`DESKTOP_BINARIES`) and `scripts/dev-runtime-binaries.sh` carry `tddy-ios-emulator`; `publish.sh`
  does not.

## Decisions

- One crate per OS (`tddy-android-emulator` later behind the same trait).
- Local host only: a simulator on another Mac is reached by that Mac's daemon.
- No fallback when a permission is missing: the start fails naming it.
- Dependencies (developer, 2026-11-26): xcap and `objc2-core-graphics` reused;
  `objc2-application-services`, `objc2`, `objc2-foundation`, `block2` for AX, the Objective-C runtime
  and XPC blocks. CoreSimulator is loaded at run time, never linked.
- Sandboxed agents reaching CoreSimulator: out of scope (developer, 2026-11-26).

## Verification

Acceptance suites in `tddy-ios-emulator` (`simctl_parse`, `geometry`, `keysyms`, `dtuhid_wire`,
`client_acceptance`, `device_listing`, `bridge_binary`), `tddy-screen-sharing`
(`ios_simulator_targets_acceptance`), `tddy-host-service` (`ios_simulator_channels`,
`ios_simulator_listing`, `host_tooling` unit tests), `tddy-daemon-kernel`
(`ios_simulator_binary_resolution`), `tddy-vnc` `wheel_buttons`, `tddy-rdp` `wheel_pdus`,
`tddy-screenshare` `rgba_to_i420`, and Cypress specs in `tddy-hosts-web`, `tddy-web` and
`tddy-client-web` / `tddy-livekit-web` unit tests. Not run: `simulator_production.rs` (`#[ignore]`,
needs a booted simulator), and the full `tddy-vnc` / `tddy-rdp` suites after the signature change
(compile-checked with `--all-targets`; left to CI).

## Backlog

- **Resolved and deleted:** `2026-09-07-remote-desktop-input-forwarding` ("remote desktop input
  forwarding: the browser never sends any") — its one open part, scroll-wheel forwarding, landed here.
- **Added:** `2026-11-27-ios-simulator-no-multi-touch`, `2026-11-27-ios-simulator-scroll-has-no-momentum`,
  `2026-11-27-ios-simulator-window-read-once-at-connect`,
  `2026-11-27-ios-simulator-capture-colour-space-and-resolution`,
  `2026-11-27-ios-simulator-needs-a-visible-window`, `2026-11-27-publish-sh-does-not-ship-tddy-ios-emulator`,
  and `2026-11-27-host-tooling-rs-over-the-500-line-budget` (463 → 517 production lines accepted by the developer).
- **Kept:** `2026-10-18-screen-share-mid-session-resize` (rotation changes the simulator's size
  mid-stream) and `2026-10-20-screen-sharing-two-stand-in-bridges` (the iOS tests reuse the existing
  stand-in; no third added).
- **Code issues:** none recorded in the touched packages changed measurement.
  `tddy-host-service` `complexity-host-messages-unlock-and-add` is untouched (this PR edited
  `host_remote_desktop_message` in the same file, not `unlock_and_add`).
- An optional `tddy-tools emulator {list,boot,install,launch}` wrapper was considered and not built:
  plain `xcrun simctl` is enough.

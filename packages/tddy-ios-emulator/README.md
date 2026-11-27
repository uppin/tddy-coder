# tddy-ios-emulator

The screen-sharing bridge for a **booted iOS Simulator** on the daemon's own Mac. It streams the
device screen (cropped from its window, with no chrome) into LiveKit as a screenshare track and
turns viewer input into touches and typing on the device.

It is a `tddy_screenshare::ScreenSharingClient`, run by `tddy_screenshare::run_bridge` like
`tddy-vnc` and `tddy-rdp`. The daemon spawns it for a `PROTOCOL_IOS_SIMULATOR` host target.

Architecture: [docs/architecture.md](docs/architecture.md). Product behaviour:
[screen-sharing-sessions.md](../../docs/ft/web/screen-sharing-sessions.md) § iOS Simulator.

## Modes

| Invocation | stdin | stdout |
|---|---|---|
| `tddy-ios-emulator` | a `BridgeConfig` JSON whose `device` is the simulator udid (`host` loopback, `port` 0) | one start-report line (`{"ready":{…}}` or `{"failed":{…}}`), then nothing |
| `tddy-ios-emulator --inspect` | an `InspectTarget` | a report that the simulator advertises no sign-in |
| `tddy-ios-emulator --list-devices` | nothing | one JSON line: `[{"udid","name","runtime"}, …]` of the **booted** devices; on failure a reason on stderr and a non-zero exit |

`tddy-host-service` runs `--list-devices` to offer simulators on the Hosts screen, so it takes no
dependency on this crate's LiveKit stack.

## Platforms

The crate builds everywhere. Off macOS `connect` fails with "iOS Simulator `<udid>` can only be
bridged on macOS" and `--list-devices` fails, so a host lists no simulators. `./release`,
`./install` and `./install --desktop` build and ship the binary beside the daemon on every platform.
`publish.sh` does not ship it.

## Requirements (macOS)

- Xcode, selected with `xcode-select`. The system `/usr/bin/xcrun` is run with `DEVELOPER_DIR` and
  `SDKROOT` removed, so a nix dev shell's toolchain `xcrun` is never used.
- **Screen Recording** and **Accessibility** granted to the process that runs the bridge (the
  daemon, or Tddy Desktop). A missing permission fails the start with a message naming it.
- The device booted, with its window **visible and not minimised** in Simulator.app (Xcode ≤ 26) or
  Device Hub (Xcode 27) on the logged-in console.

## Tests

`./test -p tddy-ios-emulator`. Everything runs without a Mac or Xcode: ports are in-memory
(`tests/common`), and `tests/bridge_binary.rs` spawns the real binary against an unknown udid.
`tests/simulator_production.rs` is `#[ignore]`d — it needs a booted simulator and the two
permissions, and runs by hand on a Mac.

## Attribution

`src/dtuhid.rs` and `src/dtuhid_xpc.rs` port the DTUHID input protocol from
[meta/idb](https://github.com/meta/idb) (MIT). See [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES).

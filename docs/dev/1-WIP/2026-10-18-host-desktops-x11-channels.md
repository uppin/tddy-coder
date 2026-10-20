# Changeset: One desktop channel per X11 display

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#659](https://github.com/uppin/tddy-coder/pull/659)
**Type**: Feature
**Stack**: `#host-desktops` 7/8. Base: `feature/host-desktops/bridge-ready`.

## Initial Discovery

[2026-10-18-host-desktops-x11-channels-initial-discovery.md](./2026-10-18-host-desktops-x11-channels-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-host-service/docs/code-issues/oversized-file-host-tooling.md` | ⚠ DURING | Enumeration goes in a new module (`x11_displays.rs`); `host_tooling.rs` only swaps `PROBED_PROTOCOLS` iteration for a channel list. Record the delta. |
| `packages/tddy-screen-sharing/docs/code-issues/oversized-file-screen-sharing-service.md` | ⚠ DURING | The x11vnc lifecycle goes in its own module beside node 1's bridge-lifecycle module. |
| `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` | ⚠ DURING | Two fields added to `ScreenSharingConfig`; keep the resolver beside the existing two. |

## Responsibility

- `host.proto` `HostRemoteDesktop` channel identity: `channel_id`, `DesktopChannelKind kind` (`ENDPOINT`, `X11_DISPLAY`), `uint32 display`, `string label`.
- `tddy-host-service/src/x11_displays.rs` (new): list `X<N>` sockets in the configured directory that the daemon user can connect to; drop a display already served on `5900+N`; report `can_bridge` from `x11vnc` resolution.
- `ScreenSharingConfig.x11vnc_binary_path`, `.x11_socket_dir`, `resolve_x11vnc_binary_path`.
- Host targets for X11 displays (`HostDesktopTarget.display: Option<u32>`), and the x11vnc lifecycle: spawn `x11vnc -display :N -localhost -rfbport <free> -nopw -forever` before the bridge, wait until it accepts, point the bridge at it; stop it on `StopHostStream`, on bridge exit, and before a reopen.
- Web: `HostRowRemoteDesktop` lists every channel with its own Connect; `HostDesktopOverlay` opens the channel it is given (no `PROBED_HOST`/fixed-port matching).
  - ⚠️ Delivered for X11 channels (matched by `(protocol, x11Display)`) and for the port (always the reading's). **Endpoint** channels still match and add their target on `127.0.0.1`, which is where the endpoint probe found them: kept on purpose, recorded as `docs/dev/todo/2026-10-20-endpoint-channel-targets-matched-by-loopback-port.md`.

## Boundaries

- No X protocol traffic, no other users' displays, no Wayland, no new X servers.
- Does not change sign-in (x11vnc is loopback `-nopw`, signed in with None); node 8 owns the form.
- Does not allow several open desktops at once.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `service-split` | host-target RPC module, bridge-lifecycle module | adds the x11vnc step to the host start/stop there | move modules |
| n4 `hosts-mount` | tooling cell on the Hosts screen; probe cache | channels render through it; the cache caches the channel list | change the cache |
| n6 `bridge-ready` | `spawn_bridge` → `BridgeStarted`; failure mapping | the bridge of an X11 channel is started and judged by `spawn_bridge` unchanged. x11vnc's own failures are mapped in `x11vnc.rs`, **not** by n6's `await_bridge_readiness`, to the same status codes n6 uses for the matching bridge failures: never accepts within the window → `DeadlineExceeded`, exits before accepting → `Unavailable`, cannot be spawned → `Internal`. No `Unauthenticated` path: x11vnc is `-nopw` | change readiness |

**Sequencing fact:** wave 2 — needs node 1 green (module layout).

## Draft PR contract

```proto
// host.proto
enum DesktopChannelKind { DESKTOP_CHANNEL_KIND_UNSPECIFIED = 0; DESKTOP_CHANNEL_KIND_ENDPOINT = 1; DESKTOP_CHANNEL_KIND_X11_DISPLAY = 2; }
message HostRemoteDesktop { …existing…; string channel_id = 7; DesktopChannelKind kind = 8; uint32 display = 9; string label = 10; }
// screen_sharing.proto
message AddHostTargetRequest { …; optional uint32 x11_display = 8; }
```
```rust
// tddy-host-service/src/x11_displays.rs
pub struct X11Display { pub number: u32, pub socket: PathBuf }
pub fn list_x11_displays(socket_dir: &Path) -> std::io::Result<Vec<X11Display>>;
// tddy-daemon-kernel config
pub fn resolve_x11vnc_binary_path(config: &ScreenSharingConfig) -> PathBuf;
```
Generated TS committed. Failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** **no** — waits on node 1 green (module layout). Tests use stand-in `x11vnc` scripts and a temp socket dir, not node 6's behaviour beyond its published signature.
**Concurrent with:** node 6
**Blocks:** nothing.

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-service` (proto), `tddy-host-service`, `tddy-daemon-kernel`, `tddy-screen-sharing`, `tddy-web`

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-x11-channels.md`

## Summary

A host with several X displays offers each as a desktop.

## Scope

- [x] Testing — acceptance and unit tests green (scoped; see Validation Results)

- [x] Proto channel identity; regenerated TS
- [x] Display enumeration + config
- [x] x11vnc lifecycle around the bridge
- [x] Row lists channels; overlay opens a given channel

## Technical Changes

### State A
- `remote_desktop_probe.rs:29-31,104` fixed `5900`/`3389` on `127.0.0.1`; `host_tooling.rs:202,370` `PROBED_PROTOCOLS` × `default_port()`.
- `HostRowRemoteDesktop.tsx:70-81` `connectableDesktop` returns the first match; `HostDesktopOverlay.tsx:55,71-95` hard-codes `127.0.0.1` and matches targets by `(host, port, protocol)`.
- `host_desktop_targets.rs:33-42` target has no display.
- `config.rs:616-660` `ScreenSharingConfig { vnc_binary_path, rdp_binary_path }`.

### State B
Tooling returns endpoint readings plus one reading per X11 display; host targets may name a display;
starting one runs x11vnc then the bridge.

### Delta
| Package | Change |
|---|---|
| `tddy-service` | `host.proto`, `screen_sharing.proto`; `tddy-web/src/gen`, `tddy-rust-typescript-tests/gen` |
| `tddy-host-service` | `x11_displays.rs` (new), `host_tooling.rs`, `host_messages.rs`, `host_desktop_targets.rs` |
| `tddy-daemon-kernel` | `config.rs` |
| `tddy-screen-sharing` | x11vnc lifecycle module (new); host start/stop |
| `tddy-web` | `HostRowRemoteDesktop.tsx`, `HostDesktopOverlay.tsx`, page objects |

## Implementation Milestones
- [x] M1 Proto + enumeration + tooling
- [x] M2 x11vnc lifecycle with stand-in binary
- [x] M3 Web channels

## Testing Plan

Rust: a temp directory with Unix listeners named `X0`, `X1` (and a dead socket file) as the socket
dir; a stand-in `x11vnc` shell script that records argv and listens on the requested port (or never
does). Web: Cypress with two channel readings. No real X server or x11vnc in automated tests;
a manual check against a real `Xvfb :1` + `x11vnc` is recorded in Validation Results.

## Acceptance Tests

Daemon tests are placed outside `screen_sharing_service.rs` so node 1's split does not move them;
they drive `ScreenSharingServiceImpl` through the `ScreenSharingService` trait, reusing node 6's
`tests/common/` (extended, not duplicated).

| Status | Test | File | Validates |
|---|---|---|---|
| [x] | `lists_each_display_the_user_can_connect_to` | `packages/tddy-host-service/src/x11_displays.rs` (`tests`) | enumeration, ordered by number |
| [x] | `skips_a_display_socket_nothing_answers_on` | same | stale sockets |
| [x] | `a_host_with_no_x11_socket_directory_lists_no_displays` (added) | same | missing `/tmp/.X11-unix` is not an error |
| [x] | `does_not_list_a_display_already_served_on_its_vnc_port_twice` | `packages/tddy-host-service/src/host_tooling.rs` (`tests`) | de-dup via the injected endpoint probe on `5900+N` |
| [x] | `reports_x11_channels_as_unbridgeable_without_x11vnc` | same | `can_bridge = false` |
| [x] | `names_an_x11_channel_by_its_display` (added) | `packages/tddy-host-service/src/host_messages.rs` (`tests`) | wire `kind`, `display`, `channel_id = "x11:1"`, `label = "Display :1"` |
| [x] | `identifies_an_endpoint_channel_by_its_protocol_and_port` (added) | same | `kind = ENDPOINT`, `channel_id = "vnc:5900"` |
| [x] | `resolves_x11vnc_to_the_configured_path` (added) | `packages/tddy-daemon-kernel/tests/x11vnc_binary_resolution.rs` | resolver |
| [x] | `resolves_an_unconfigured_x11vnc_to_a_path_lookup_of_its_bare_name` (added) | same | resolver default |
| [x] | `opening_an_x11_channel_starts_x11vnc_for_that_display_on_loopback_only` | `packages/tddy-screen-sharing/tests/x11_channels_acceptance.rs` (moved from the x11vnc module) | argv `-display :1 -localhost` |
| [x] | `the_bridge_of_an_x11_channel_connects_to_its_x11vnc` (added) | same | bridge config `host 127.0.0.1`, `port` = x11vnc `-rfbport` |
| [x] | `closing_an_x11_channel_stops_its_x11vnc` | same | stop |
| [x] | `reopening_an_x11_channel_leaves_one_x11vnc_running` | same | no orphan |
| [x] | `reopening_an_x11_channel_whose_old_bridge_exits_with_its_x11vnc_keeps_the_new_x11vnc` (added, /validate-changes) | same | the old bridge's exit does not stop the reopened channel's x11vnc |
| [x] | `an_x11vnc_that_never_accepts_fails_the_start_and_starts_no_bridge` | same | failure path (`DeadlineExceeded`, mapped in `x11vnc.rs` to the code n6 uses for a silent bridge) |
| [x] | `an_x11vnc_that_never_accepts_is_stopped` (added) | same | no orphan on failure |
| [x] | `an_x11_channel_whose_bridge_exits_stops_its_x11vnc` (added) | same | stop on bridge exit |
| [x] | `offers one connect per connectable channel` | `packages/tddy-web/cypress/component/HostsScreenDesktopChannelsAcceptance.cy.tsx` (new) | many Connects |
| [x] | `opens the channel whose connect was pressed` | same | `x11Display` sent, start on that target |
| [x] | `names each x11 channel by its display` | same | "Display :1" |

**Stand-in x11vnc:** a `/bin/sh` script that appends `<pid> <argv>` to a launch log and sleeps; the
listening half is done by a watcher thread in the test that binds `127.0.0.1:<-rfbport>` for each
launch (so the suite needs no `nc`/`perl`). Node 6's stand-in bridge now records its stdin config.

**Contract surface (published at the contract stage; every body implemented in green):**
- `tddy_host_service::x11_displays::{X11Display, list_x11_displays, DEFAULT_X11_SOCKET_DIR}`; `with_x11_channels` (crate-private)
- `remote_desktop_probe::DesktopChannel { Endpoint, X11Display(u32) }`, `DesktopReachability.channel`
- `SubprocessHostToolingProbe::listing_x11_displays_for(&DaemonConfig)` (real setter; `for_config` calls it, so the daemon's probe lists X11 channels)
- `HostDesktopTarget.display: Option<u32>` (serde default, skipped when `None`); `add_host_target` stores `req.x11_display`
- `ScreenSharingConfig.{x11vnc_binary_path, x11_socket_dir}`; `resolve_x11vnc_binary_path(&ScreenSharingConfig) -> PathBuf` (no current-exe sibling step, so not identical to the two bridge resolvers)
- `tddy_screen_sharing::x11vnc::{X11vncServers, X11vncStarted, x11vnc_arguments, DEFAULT_X11VNC_ACCEPT_TIMEOUT}`; `ScreenSharingServiceImpl::with_x11vnc_accept_timeout`
- Web: `connectableChannels(readings)`, `ConnectableDesktop.{channelId, x11Display}`, `HostDesktopOverlay` prop `x11Display`; page object `hostDesktopChannelsPage` (Connect carries `data-channel-id`, channel reading `hosts-row-<id>-channel-<channelId>`)

**Resolved in the contract (2026-10-18):** `ListHostTargets` returns `ScreenSharingTarget`, which had no
display, so the overlay could not find an existing X11 target and would add one per open.
`ScreenSharingTarget` gains `optional uint32 x11_display = 7`; `host_target_to_proto` maps `HostDesktopTarget.display`
into it (a pass-through), session-scope literals set `None`. Green matches an existing X11 target by
`(protocol, x11_display)` in `HostDesktopOverlay`.

## Decisions & Trade-offs
- **Daemon-spawned x11vnc per display, daemon user's displays only, loopback `-nopw`** (developer, 2026-10-18).
- **Keep endpoint probes** beside X11 channels so macOS and RDP hosts keep working (developer, 2026-10-18).
- **New runtime dependency `x11vnc`** — optional; absence is reported, never a failure (no fallback).

## Technical Debt & Production Readiness

## Refactoring Needed
### From @red (TDD Red Phase)
- `AnX11SocketDir` lives in `tddy-host-service/src/test_util.rs` (shared by `x11_displays` and `host_tooling` tests).
- `tests/common/mod.rs` (screen-sharing) grows `StandInX11vnc`/`TheX11vnc`; it still duplicates the inline suite's fixtures (node 6's note) — fold together once node 1 splits the service file.
- `host_tooling.rs` grew by ~150 lines (mostly tests); record the delta against `oversized-file-host-tooling.md`.
### From /validate-changes
### From /validate-tests
- Split `an_x11_channel_whose_bridge_exits_stops_its_x11vnc` into the failed-start case (renamed) and
  a bridge that reports ready then exits on its own.
- One equality assertion in `reports_x11_channels_as_unbridgeable_without_x11vnc`.
- Add the non-display-entry test to `x11_displays.rs` and the unbridgeable-X11 Cypress case; drop the
  endpoint fixture's invented label.
### From /validate-prod-ready
- None beyond the deferred socket-directory error (TODO above).
### From /analyze-clean-code
- Extract the X11 block of `start_host_stream_impl`; split `with_x11_channels`; `ChannelReading`
  sub-component; name the loopback host; name the overlay's target predicate.

## Validation Results
### /validate-changes

**Run 2026-10-20 ([W] stage).** Risk summary: Critical 0 · Warning 3 · Info 3.

**Stack gate.** Stack branch (planned; base `feature/host-desktops/bridge-ready`, PR #658 open).
`/pr-stack-rebase` first verified the branch current on `578693a14`. n6 then pushed `31af5b866`
(bridge readiness moved to `tddy_daemon_extensions::bridge_readiness`), so the branch was rebased
`--onto origin/feature/host-desktops/bridge-ready 578693a14` (backup branch kept). The one conflict,
in `tests/common/mod.rs` imports, was resolved by keeping n6's `tddy_daemon_extensions` import and
dropping the old `tddy_screenshare::readiness` line, and adding this PR's `DEFAULT_X11VNC_ACCEPT_TIMEOUT`.
Leak check: `origin/<base>..HEAD` is this PR's four commits only. No deletions, no Cargo changes.

**Stack boundary.** Responsibility delivered except the endpoint-channel half of the overlay item
(⚠️, deliberate: see Responsibility and the TODO). Dependencies: nothing of n1/n4/n6 implemented
here; `spawn_bridge` and `await_bridge_readiness` unchanged. Boundaries respected (no X protocol, no
sign-in change, one open desktop). Diff contains only this PR's files.

**Build / tests (scoped, after the rebase).** `./test -p tddy-screen-sharing`: 62 passed, 0 failed.
`./test -p tddy-host-service`: 198 passed, 0 failed. `cargo test -p tddy-daemon-kernel --test
x11vnc_binary_resolution`: 2 passed. `cargo clippy -p <pkg> --all-targets -- -D warnings` clean for
tddy-screen-sharing, tddy-host-service and tddy-daemon-kernel. Cypress, 6 specs
(`HostsScreenDesktopChannelsAcceptance`, `HostDesktopConnectAcceptance`,
`HostDesktopStartFailureAcceptance`, `HostsScreenRemoteDesktopAcceptance`,
`HostsScreenToolingMountAcceptance`, `ScreenSharingInputScopesAcceptance`): 29 passed, 0 failed.

**Findings.**
- [WARNING, fixed] `an_x11vnc_that_never_accepts_is_stopped` was load-sensitive. Cause: the
  stand-in x11vnc's first exec under load outlasted the 500 ms accept window, so the daemon killed
  it before it logged its launch and `launch(0)` waited on a launch that could never appear. Fix:
  the stand-in is warmed once before any test clock starts (n6's `StandInBridge::warm_up` pattern),
  and a further failure (1 in 7 runs at load ~85, the warmed `/bin/sh` still missing 500 ms) set
  `A_SHORT_ACCEPT_WAIT` to 3 s, with the reason recorded beside it. Afterwards: 3 of 3 suite runs green.
- [WARNING, sent to /green] Reopen race: `start_host_stream` started the new x11vnc (stopping the
  old one) before `spawn_bridge` terminated the old bridge. A real bridge exits when its VNC
  server goes, and its reaper, still finding its pid under the key, stopped the **new** x11vnc.
  **Fixed via /green (tdd-implementer):** `start_host_stream` terminates the old bridge before
  starting the x11vnc. New test `reopening_an_x11_channel_whose_old_bridge_exits_with_its_x11vnc_keeps_the_new_x11vnc`
  (stand-in bridge `TheBridge::ready_at_until_its_x11vnc_stops`) failed 4/4 before the fix (`Unavailable:
  x11vnc for display :1 exited before accepting: signal: 15`), passes 3/3 after; `./test -p
  tddy-screen-sharing` 63 passed, 0 failed; clippy clean.
- [WARNING, deferred] An unreadable X11 socket directory (anything but `NotFound`) is logged and
  lists no X11 channels, so the host shows no displays rather than a failed reading. No test or AC
  pins that path; the honest fix needs a channel identity `host.proto` does not have:
  `docs/dev/todo/2026-10-20-x11-socket-dir-read-failure-lists-no-channels.md`.
- [INFO] Endpoint channels keep `(127.0.0.1, port, protocol)` target matching:
  `docs/dev/todo/2026-10-20-endpoint-channel-targets-matched-by-loopback-port.md`.
- [INFO] The Dependencies row said x11vnc failures go "through the same mapping" as n6's. They do
  not: `x11vnc.rs` maps them itself to the same codes. Row corrected.
- [INFO] `X11vncServers::stop` sends SIGTERM and does not wait; the reaper task collects the exit.
  Same shape as `terminate_bridge`.

**Re-run (`/pr-wrap` step 1, head `4e5092572`).** Stack gate: current on `31af5b866`, leak-free (five
own commits). The gap fix and its fixture (`TheBridge::ExitsWithItsX11vnc`, `keeps_running`,
`has_exited_as`) reviewed: no new findings; nothing for `refactor`. One note: the stand-in bridge
reads `port` out of its config with `sed`, which relies on serde_json's compact output.

**Changeset sync.** Testing `[~]` → `[x]`; contract-stage wording ("`todo!()`", "`for_config` not
yet wired") corrected to what green delivered; Responsibility web item annotated ⚠️ (endpoint
channels); Dependencies n6 row corrected.
### /validate-tests

**Run 2026-10-20.** 26 tests analyzed (Rust: `x11_displays` 3, `host_tooling` 2, `host_messages` 2,
`x11vnc_binary_resolution` 2, `x11_channels_acceptance` 8; Cypress `HostsScreenDesktopChannelsAcceptance` 3;
plus the fixture changes in `tests/common/mod.rs` and `test_util.rs`). Critical 0 · Warning 4 · Info 2.
All follow Given/When/Then, names read as behaviour, page objects used in the Cypress spec, no `.only`,
`.skip` or `#[ignore]`.

- [WARNING, description-body misalignment] `x11_channels_acceptance.rs::an_x11_channel_whose_bridge_exits_stops_its_x11vnc`
  uses `TheBridge::ExitsBeforeReporting`, so the *start* fails and the x11vnc is stopped by the
  failed-start path in `start_host_stream`. The path its name describes, a bridge that came up and
  then exits (`reap_bridge` → `x11vnc.stop`), is untested. It also discards the start's result.
- [WARNING, assertion roulette] `host_tooling.rs::reports_x11_channels_as_unbridgeable_without_x11vnc`:
  four separate asserts (count, channel, reachable, can_bridge) for one reading.
- [WARNING, missing edge case] `x11_displays.rs`: no test that entries not named `X<digits>`
  (`X1-lock`, `X`, `.X1-lock`) are skipped, though `display_number_of` exists for exactly that.
- [WARNING, missing edge case] Cypress: PRD success criterion "Without `x11vnc` the channels are
  listed but offer no Connect" has no web test for an X11 reading with `canBridge: false`.
- [INFO, test data] `aVncServerOnItsDefaultPort()` sets `label: "VNC :5900"`, which the daemon never
  sends for an endpoint channel (`host_remote_desktop_message` leaves it empty).
- [INFO, timing] `StandInX11vnc::keeps_running` is a fixed-window negative check (1 s); justified in
  its doc comment, since a stop that is not coming cannot be awaited.
### /validate-prod-ready

**Run 2026-10-20: ✅ Ready.** 14 production files checked (tests, Cypress and `src/gen` excluded).
Blockers 0 · Warnings 1 (already deferred). Mock code 0, TODO/FIXME/HACK 0, debug output 0
(`println!`/`eprintln!`/`dbg!`/`console.*` none; x11vnc's stderr is inherited by the daemon exactly as
the bridge's is in `start_bridge_process`).

- [WARNING, deferred] `x11_displays.rs::with_x11_channels` turns a non-`NotFound` socket-directory
  error into a `log::warn!` and no X11 channels: recorded as
  `docs/dev/todo/2026-10-20-x11-socket-dir-read-failure-lists-no-channels.md`.
- Acceptable: `config.screen_sharing.clone().unwrap_or_default()` (`host_tooling.rs`,
  `host_targets.rs`) is the documented meaning of an absent `screen_sharing:` block, the same as the
  bridge resolvers'; `child.id().unwrap_or(0)` in `x11vnc.rs` is the zero sentinel `start` never
  records, as `record_bridge_pid` does; the `5900` probe in `with_x11_channels`'s `unwrap_or_else`
  is only for a display number past `u16` and asks for the bridge half only.
- Unused code: none. `x11vnc_arguments`, `X11vncStarted`, `DEFAULT_X11_SOCKET_DIR` are used in-crate
  and part of the published contract surface.
### /analyze-clean-code

**Run 2026-10-20: score C before cleanup, B after** (1 must-refactor, 3 needs-attention, 4 minor).

| Metric | Excellent | Acceptable | Needs attention | Must refactor |
|---|---|---|---|---|
| File length (≤500 / >500) | — | 15 | 2 (`config.rs` 1,575, `host_tooling.rs` 806; both pre-existing, grown here; see 3.5 below) | — |
| Function length (functions this PR wrote or grew) | 17 | 6 | 2 | 1 |
| Nesting depth | all ≤ 3 | — | — | — |
| Parameter count | all ≤ 3 | — | — | — |

- [MUST REFACTOR] `screen_sharing_service/host_targets.rs::start_host_stream_impl`: 67 → 90 lines.
  The X11 block (start x11vnc, repoint the bridge, stop x11vnc on a failed spawn) is one helper.
  The file is not touched by #660, so extracting a method here conflicts with nobody.
- [NEEDS ATTENTION] `x11_displays.rs::with_x11_channels`: 52 lines. The socket-dir default and the
  per-display reading are two helpers.
- [NEEDS ATTENTION] `HostRowRemoteDesktop.tsx` component: 60 lines (≈50 before), the per-reading JSX
  is a `ChannelReading` sub-component.
- [NEEDS ATTENTION, pre-existing] `HostDesktopOverlay.tsx` component: ~200 lines, n4's; this PR
  added one prop and one dependency. Not changed here.
- [MINOR, magic value] `"127.0.0.1"` three times in `x11vnc.rs` and once in `host_targets.rs`.
  Name it once, in `x11vnc.rs` (`X11VNC_HOST`), and use it for the bridge's host too.
- [MINOR, readability] `HostDesktopOverlay.tsx::targetForChannel`: the nested-ternary `find`
  predicate becomes a named `isTargetOfChannel`.
- [MINOR, duplication] `"Display :N"` is built in `host_messages.rs` (label) and again in
  `HostDesktopOverlay.tsx` (target label). The web could reuse the reading's `label`, but the
  overlay is given a display, not a reading. Kept: two layers, one format string each.
- [MINOR, duplication] `x11vnc.rs::{reap, forget}` mirror `bridge_lifecycle.rs::{reap_bridge,
  forget_bridge_pid}`. A shared process table would change n1's bridge-lifecycle module. Kept,
  noted for the post-stack restructure.

**After `refactor` (score B).** `start_host_stream_impl` 90 → 70 (its X11 block is
`serve_x11_display_to_the_bridge` and `spawn_host_bridge`). Not taken further: what is left is the
pre-existing 67-line body, and n6 (#658, open below this PR) touches `host_targets.rs` and records
that function in `docs/dev/todo/2026-10-20-screen-sharing-start-functions-over-length.md`, so the
parent's target lookup was not extracted here. `with_x11_channels` 52 → 25 (`socket_dir_of`,
`x11_channel_reading`); `HostRowRemoteDesktop` 60 → 39 (`ChannelReading`); `X11VNC_HOST` named
once; `isTargetOfChannel` named. `./test -p tddy-screen-sharing` 64 passed, 0 failed;
`./test -p tddy-host-service` 198 passed, 1 failed: `github_cli_environment::the_gh_asked_as_a_user_is_given_none_of_the_daemons_github_tokens`
(`gh … did not answer within 5s`), a load timeout in a test this PR does not touch, which passed
re-run alone (1 passed). Clippy clean on both; Cypress 4 specs 25 passed, 0 failed.

**File length gate (3.5).** `config.rs` 1,557 → 1,575 and `host_tooling.rs` 775 → 806, both
already over budget and grown here; neither touched by #660 or any lower node. Not decomposed:
this changeset's `## Prerequisites` verdicts (⚠ DURING, "record the delta", "keep the resolver
beside the existing two"), approved with the plan, and `oversized-file-config.md`'s own
"do not absorb this into a feature node". Growth recorded in both code-issue records'
measurement history. Every other non-test file is under 500 (`x11_displays.rs` 144 and
`x11vnc.rs` 198 are new).

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-x11-channels-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-x11-channels-initial-discovery.md`
- [ ] USER REVIEW — work complete

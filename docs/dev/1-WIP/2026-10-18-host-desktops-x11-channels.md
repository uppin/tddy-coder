# Changeset: One desktop channel per X11 display

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: (opened in wave 1; number recorded in wave 2)
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

## Boundaries

- No X protocol traffic, no other users' displays, no Wayland, no new X servers.
- Does not change sign-in (x11vnc is loopback `-nopw`, signed in with None); node 8 owns the form.
- Does not allow several open desktops at once.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `service-split` | host-target RPC module, bridge-lifecycle module | adds the x11vnc step to the host start/stop there | move modules |
| n4 `hosts-mount` | tooling cell on the Hosts screen; probe cache | channels render through it; the cache caches the channel list | change the cache |
| n6 `bridge-ready` | `spawn_bridge` → `BridgeStarted`; failure mapping | an x11vnc that never accepts is reported through the same mapping | change readiness |

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

- [ ] Proto channel identity; regenerated TS
- [ ] Display enumeration + config
- [ ] x11vnc lifecycle around the bridge
- [ ] Row lists channels; overlay opens a given channel

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
- [ ] M1 Proto + enumeration + tooling
- [ ] M2 x11vnc lifecycle with stand-in binary
- [ ] M3 Web channels

## Testing Plan

Rust: a temp directory with Unix listeners named `X0`, `X1` (and a dead socket file) as the socket
dir; a stand-in `x11vnc` shell script that records argv and listens on the requested port (or never
does). Web: Cypress with two channel readings. No real X server or x11vnc in automated tests;
a manual check against a real `Xvfb :1` + `x11vnc` is recorded in Validation Results.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `lists_each_display_the_user_can_connect_to` | `packages/tddy-host-service/src/x11_displays.rs` (`tests`) | enumeration |
| `skips_a_display_socket_nothing_answers_on` | same | stale sockets |
| `does_not_list_a_display_already_served_on_its_vnc_port_twice` | `packages/tddy-host-service/src/host_tooling.rs` (`tests`) | de-dup |
| `reports_x11_channels_as_unbridgeable_without_x11vnc` | same | `can_bridge = false` |
| `opening_an_x11_channel_starts_x11vnc_for_that_display_on_loopback_only` | `packages/tddy-screen-sharing` x11vnc module `tests` | argv `-display :1 -localhost` |
| `closing_an_x11_channel_stops_its_x11vnc` | same | stop |
| `reopening_an_x11_channel_leaves_one_x11vnc_running` | same | no orphan |
| `an_x11vnc_that_never_accepts_fails_the_start_and_starts_no_bridge` | same | failure path |
| `offers one connect per connectable channel` | `packages/tddy-web/cypress/component/HostsScreenDesktopChannelsAcceptance.cy.tsx` (new) | many Connects |
| `opens the channel whose connect was pressed` | same | `x11Display` sent |
| `names each x11 channel by its display` | same | "Display :1" |

## Decisions & Trade-offs
- **Daemon-spawned x11vnc per display, daemon user's displays only, loopback `-nopw`** (developer, 2026-10-18).
- **Keep endpoint probes** beside X11 channels so macOS and RDP hosts keep working (developer, 2026-10-18).
- **New runtime dependency `x11vnc`** — optional; absence is reported, never a failure (no fallback).

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-x11-channels-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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

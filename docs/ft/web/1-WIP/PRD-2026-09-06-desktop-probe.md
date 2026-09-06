# PRD — Hosts screen: VNC and RDP reachability per host

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web` (screen) + `daemon` (probe)
**Stack:** `#hosts-screen` node 7 of 8
**Branch:** `feature/hosts-screen/desktop-probe` → base `feature/hosts-screen/agent-add-key`

## Affected features

| Document | Relationship |
|---|---|
| [`docs/ft/web/screen-sharing-sessions.md`](../screen-sharing-sessions.md) | The existing per-session VNC/RDP feature; this node reports host-level reachability for it |
| [`PRD-2026-09-06-host-identity.md`](./PRD-2026-09-06-host-identity.md) | Owns the tooling probe RPC this node adds a block to |

## Summary

Report, per host, whether a **remote desktop is reachable** over VNC or RDP — and, separately, whether
that host's daemon is even **able to bridge** one.

## Background

tddy already bridges both protocols: `packages/tddy-vnc` (RFB, via `vnc-rs`) and `packages/tddy-rdp`
(IronRDP) are outbound bridge binaries that republish a remote framebuffer as a LiveKit video track,
driven by `ScreenSharingService`. What does not exist is any notion of **availability**: no port probe,
no reachability check, and — notably — not even an `exists()` check on the bridge binary.

`resolve_vnc_binary_path` / `resolve_rdp_binary_path` (`packages/tddy-daemon/src/config.rs:451-500`)
resolve a path by *guessing*: explicit config → sibling of `current_exe()` → bare name on `PATH`. A
missing binary is discovered only when a spawn fails at `screen_sharing_service.rs:174` — after the
user has already asked for a stream.

## Proposed changes

### Two availabilities, reported separately

| Fact | Meaning | Probe |
|---|---|---|
| **Bridge capability** | This host's daemon can spawn a VNC/RDP bridge | The resolved bridge binary actually exists |
| **Desktop reachability** | Something is serving a desktop on this host | A bounded TCP connect to the protocol's port |

⚠ These must not be collapsed. "tddy cannot bridge here" and "nothing is serving a desktop here" are
different problems with different fixes.

### The probe is non-intrusive and bounded

- A **TCP connect that closes immediately** — never a protocol handshake. A half-open RFB handshake
  against someone's desktop is not a thing a monitoring screen should do on a timer.
- A short timeout; an unreachable host must never hold the tooling RPC open.
- The probe reports **which port it checked**, so "unavailable" is not mistaken for authoritative when a
  host serves on a non-default port.

### What is staying the same

- **`ScreenSharingService` is untouched.** Its per-session targets, its vault and its bridge spawning
  all keep working exactly as they do. This node reports; it starts nothing.
- **The `Protocol` enum is reused**, not redefined — `screen_sharing.proto:19-23` already has
  `{ UNSPECIFIED, VNC, RDP }`.
- No host-scoped target model is created here; node 8 owns that.

## Impact analysis

### Technical

- Adds a block to node 4's tooling probe response rather than a third near-identical RPC.
- The `exists()` check on the bridge path is the cheapest real improvement in this node: it converts a
  post-hoc spawn error into a fact the operator can see beforehand.
- Probes run per host per screen refresh — they must be cheap, bounded and non-intrusive.

### User

- Each host row says whether a desktop is reachable and whether tddy could bridge it, with the port
  that was checked.

## Acceptance criteria

- [ ] **AC-1** A host serving VNC on the checked port reports VNC reachable.
- [ ] **AC-2** A host serving RDP on the checked port reports RDP reachable.
- [ ] **AC-3** A host serving neither reports both unreachable, naming the ports checked.
- [ ] **AC-4** A host whose bridge binary is **missing** reports that it cannot bridge — distinctly from
      a desktop being unreachable.
- [ ] **AC-5** A probe that times out reports a probe failure, distinct from "unreachable".
- [ ] **AC-6** The probe never performs a protocol handshake — a bare TCP connect, closed immediately.
- [ ] **AC-7** The row shows the two facts separately and never conflates them.

## Out of scope for this node

Starting a stream or opening a viewer (node 8). Any host-scoped target or vault model (node 8). Any
change to `ScreenSharingService` or the per-session flow. Discovering non-default ports.

## Successor PRs

- `feature/hosts-screen/desktop-connect` — open a host's desktop in the in-app viewer.

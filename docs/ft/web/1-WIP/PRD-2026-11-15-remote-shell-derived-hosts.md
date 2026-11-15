# SSH hosts as hosts derived from a daemon - PRD

**Date**: 2026-11-15
**PRD Type**: Enhancement
**Stack**: `#remote-shell 5/7` — branch `feature/remote-shell/derived-hosts`, base `feature/remote-shell/coder-shell`

## Affected Features

- **Hosts screen**: [hosts-screen.md](../hosts-screen.md) — derived hosts listed under the daemon that reaches them
- **Create-session package**: [create-session-package.md](../create-session-package.md) — the Host field offers derived hosts
- **Daemon selector**: [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md) — LiveKit peers become one kind of host among several
- **Client ports package**: [client-ports-package.md](../client-ports-package.md) — a third host-directory source

## Summary

The machines a daemon can reach through a remote shell appear as **hosts** of their own: in the Host field when starting a session, and on the Hosts screen under the daemon that reaches them. SSH config aliases are the first kind. Choosing one starts a session whose agent runs there.

## Background

A host today is a daemon, and remote hosts are found only in the LiveKit common room. A machine without a daemon cannot be chosen, however reachable it is.

With the agent able to run on a target, the missing piece is choice: a person should pick "buildbox" the way they pick a daemon, and see where it comes from. A derived host is addressed through its parent daemon, which owns the session and serves its terminal.

## Proposed Changes

### What's Changing

- `HostService.ListDerivedHosts` answers the hosts one daemon can reach: for each, an id, a label, its kind and the target that starts a session there.
- The host directory gains a `derived` source, asked of every connected daemon. A derived host carries its parent daemon's id and uses the parent's connection.
- The Host field groups derived hosts under their parent. Choosing one sends the parent as `daemon_instance_id` and the derived host's target.
- The Hosts screen lists derived hosts beneath their parent's row, marked with their kind, with no telemetry or tooling cells.
- A session on a derived host shows that host in the session drawer.
- A parent that cannot list (unreadable SSH config) says so on its row; its other derived hosts are not hidden behind the failure.

### What's Staying the Same

- LiveKit peer daemons list, connect and start as before.
- The SSH config select for the managed `ssh_config_host` mode stays where it is.
- A derived host has no vault, no accounts and no projects of its own: those are its parent's.

## Impact Analysis

### Technical Impact

- `host.proto` gains one RPC and two messages.
- `tddy-client-web` host directory: a third source and a `parent` on `HostDescriptor`.
- No LiveKit dependency is added to any package.

### User Impact

- SSH hosts become choosable wherever a host is chosen.
- On such a session, permission prompts are answered in the terminal, and the form says so when a derived host is selected.

## Implementation Plan

1. The listing RPC.
2. The directory source and descriptor.
3. The Host field, the Hosts screen rows, the drawer badge.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] A daemon lists its SSH config aliases as derived hosts with a target each
- [ ] A derived host appears in the Host field under its parent daemon, on every surface that starts a session
- [ ] Starting on a derived host creates a session on the parent daemon with that host as its target
- [ ] The Hosts screen shows derived hosts beneath their parent, marked SSH, without daemon-only cells
- [ ] A listing failure is shown on the parent's row and is not rendered as no hosts
- [ ] A session on a derived host names it in the drawer
- [ ] An offline parent's derived hosts are not offered

## References

### Affected Features (Complete List)

- **Hosts screen**: [hosts-screen.md](../hosts-screen.md) — derived hosts listed under the daemon that reaches them
- **Create-session package**: [create-session-package.md](../create-session-package.md) — the Host field offers derived hosts
- **Daemon selector**: [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md) — LiveKit peers become one kind of host among several
- **Client ports package**: [client-ports-package.md](../client-ports-package.md) — a third host-directory source

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-derived-hosts.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`

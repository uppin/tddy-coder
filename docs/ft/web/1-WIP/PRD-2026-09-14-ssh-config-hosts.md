# List a host's SSH config Host aliases - PRD

**Date**: 2026-09-14
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Hosts screen tooling](../hosts-screen-tooling.md) — add an SSH connections cell listing OpenSSH `Host` aliases from that host's `~/.ssh/config`
- **Related Feature**: [Hosts screen](../hosts-screen.md) — the row that hangs the cell
- **Related Feature**: [Hosts screen add-key](../hosts-screen-add-key.md) — key listing stays a separate RPC; `config` remains not a key candidate
- **Successor PRs**: `feature/ssh-exec/exec` (session dropdown reuses this list), `feature/ssh-exec/split` (same list, filtered to the codebase host)

## Summary

Each tddy host exposes the OpenSSH `Host` aliases in its OS user's `~/.ssh/config`, so an operator can pick a destination the way they already pick a key for ssh-agent. This is the SSH connections menu. Later nodes in `#ssh-exec` reuse the same list as a session dropdown; they do not parse config themselves.

## Background

Managed sessions need a way to run tools on a machine that has no tddy-daemon, via SSH from the code-managing host. Today's Hosts SSH UI lists **keys in ssh-agent**, not **where ssh would connect**. `ListHostKeyCandidates` skips the file named `config`. There is no parser and no RPC for Host aliases.

## Proposed Changes

### What's Changing

- `host.HostService` gains `ListSshConfigHosts`, addressed by `daemon_instance_id` like `ListHostKeyCandidates`, so the listing runs on the host the operator is looking at.
- An in-tree parser reads that OS user's `~/.ssh/config`, honors `Include`, skips wildcard `Host` patterns, and returns explicit aliases in stable order.
- An unreadable or unparseable config is a **failure**, not an empty list. Empty means "no Host aliases".
- The Hosts row grows an SSH connections cell that shows those aliases (or empty, or could-not-check), distinguishable the way ssh-agent states already are.

### What's Staying the Same

- ssh-agent probe, add-key, and `ListHostKeyCandidates` are unchanged. `config` is still not a key candidate.
- No session start field, no tool execution, no git Serve change.
- No new crate. No russh. No browser-side parse of `~/.ssh/config`.

## Impact Analysis

### Technical Impact

- Proto + generated clients (`tddy-service`, `tddy-web` `host_pb.ts`).
- Parser + handler in `tddy-host-service`, peer-forwarded like key listing.
- Hosts UI cell next to `HostRowSshAgent`.

### User Impact

- Hosts screen shows which SSH aliases that host's config defines.
- A failed read says so; it does not look like "this host has no SSH destinations".
- Session SSH execution is **not** in this PR.

## Implementation Plan

1. Parser + unit tests over fixture configs (`Include`, wildcards, empty, unreadable).
2. `ListSshConfigHosts` RPC + handler tests (addressing, os_user, failure vs empty).
3. Hosts cell + Cypress component test distinguishing listed / empty / failed.

## Acceptance Criteria

- [ ] `ListSshConfigHosts` on a host whose config defines `Host buildbox` and `Host *` returns only `buildbox` ([Hosts screen tooling](../hosts-screen-tooling.md))
- [ ] An unreadable `~/.ssh/config` is reported as a failure, not as zero aliases ([Hosts screen tooling](../hosts-screen-tooling.md))
- [ ] A missing or empty config with no `Include` hits is an empty alias list, distinct from failure ([Hosts screen tooling](../hosts-screen-tooling.md))
- [ ] The Hosts row SSH connections cell shows the aliases for that row's host, and a failed probe cannot be mistaken for "no connections" ([Hosts screen](../hosts-screen.md))
- [ ] `Include` paths are followed; a wildcard-only config yields an empty list ([Hosts screen tooling](../hosts-screen-tooling.md))

## References

### Affected Features (Complete List)

- [hosts-screen-tooling.md](../hosts-screen-tooling.md) — SSH connections cell
- [hosts-screen.md](../hosts-screen.md) — row
- [hosts-screen-add-key.md](../hosts-screen-add-key.md) — key listing unchanged

### Related Documentation

- Stack: `#ssh-exec` 1/4; successor branch `feature/ssh-exec/exec`
- Changeset: [2026-09-14-ssh-config-hosts.md](../../../dev/1-WIP/2026-09-14-ssh-config-hosts.md)

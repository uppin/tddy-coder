# Derived hosts are offered under the daemon the web addresses - PRD

**Date**: 2026-11-16
**PRD Type**: Bug Fix
**Stack**: `#remote-shell 9/11` — branch `feature/remote-shell/derived-host-identity`, base `feature/remote-shell/sweep`

## Affected Features

- **Hosts screen**: [hosts-screen.md](../hosts-screen.md) — derived rows are asked through the id the daemon answers on
- **Create-session package**: [create-session-package.md](../create-session-package.md) — the Host field groups derived hosts under the daemon the web addresses and shows a parent's listing failure

## Summary

A derived host names its parent by the id the web addresses that daemon with — the routing id, which differs from the durable one when the daemon appends its startup timestamp — and the New-session form lists each daemon's derived hosts on its own: one daemon that never answers withholds nothing, an earlier answer is kept until a newer one arrives, a parent is asked again when it connects, when an account is connected and when the vault opens, and a parent's failure is shown beside the Host field. The Hosts screen asks an online row's daemon through the routing id it carries.

## Background

The first real deployment (Tddy Desktop, `daemon_instance_id_append_startup_timestamp: true`, a
real Coder deployment, a second daemon in the room) showed Coder workspaces on the Hosts screen and
none in the New-session form, in two steps:

1. With the flag on, nothing: `ListDerivedHosts` stamps `parent_instance_id = local_base_instance_id_for_config`
   (the durable id) while the web addresses the daemon by `local_instance_id_for_config` (the routing
   id, carried by the common-room advertisement and `/api/config`). The form groups derived hosts by
   `d.parent === daemon.instanceId`, so the group is empty; the Hosts screen's rows carry durable ids
   from `ListKnownHosts`, so `useHostClient(HostService, durableId)` resolves no connection and the
   derived rows ask nothing — and show no failure.
2. With the flag off, the Hosts screen lists the workspaces and the form still does not: the web's
   derived directory source asks every connected daemon once with `Promise.all` and keeps only the run
   in which every daemon answered. A second daemon that never answers withholds the first one's hosts;
   nothing re-asks after an account is connected or the vault opens; the failure is never shown.

## Proposed Changes

### What's Changing

- `KnownHostEntry` gains `routing_instance_id`: the id an online daemon answers RPCs on (empty offline)
- `ListDerivedHosts` stamps `parent_instance_id` with the routing id, the same id `GetHostTooling` answers with
- the web's derived directory source asks each connected parent independently, with a deadline per call, keeps a parent's last answer until a newer one arrives, records a parent's failure, and re-asks a parent when it becomes connected
- a `refreshDerivedHosts` port on the clients port: the Accounts screen calls it after an account is connected, the vault prompt after an unlock
- the New-session Host field shows a parent's listing failure beside the field (`create-session-derived-hosts-failure-<parent>`)
- the Hosts screen asks an online row's derived hosts and tooling through its `routing_instance_id`

### What's Staying the Same

- derived host ids (`<parent>/ssh/<alias>`, `<parent>/coder/<account>/<owner>/<workspace>`) keep their shape; `<parent>` is now the routing id
- `ListKnownHosts` rows keep their durable `instance_id`; the registry files nothing new
- the daemon's Coder and SSH sources, their usability rules and their reasons
- what the form submits: the parent as `daemon_instance_id`, the host's `target`

## Impact Analysis

### Technical Impact

- one proto field (`host.proto` `KnownHostEntry.routing_instance_id = 9`) and its generated TypeScript
- `tddy-client-web/src/hostDirectory/derivedSource.ts` is rewritten around one parent at a time; `tddy-web`'s `DerivedHostDirectorySources` keeps a map of answers per parent
- a new context on the clients port (`DerivedHostsRefresh`) with a null default, so a page without the provider still renders
- `tddy-hosts-web` row components take the routing id as the id they ask through

### User Impact

- a Coder workspace or SSH alias appears in the New-session form on every deployment, flag on or off, with or without a second daemon in the room
- connecting an account or unlocking the vault makes its hosts appear without a reload
- a daemon that cannot list its hosts says so beside the Host field, instead of hiding every daemon's hosts

## Implementation Plan

1. `tddy-service`: the proto field; regenerate the TypeScript
2. `tddy-host-service`: the routing id as parent; `ListKnownHosts` fills `routing_instance_id` for online rows (this daemon's own, and a peer's from the eligible roster)
3. `tddy-client-web`: per-parent asking and merging; the refresh port
4. `tddy-web`: the directory source keeps answers per parent, re-asks on connect and on refresh
5. `tddy-session-create-web`: the failure beside the Host field
6. `tddy-hosts-web`: rows ask through the routing id; `tddy-workspace-web`: Accounts and the vault prompt call the refresh

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] With `daemon_instance_id_append_startup_timestamp: true`, every derived host's `parent_instance_id` equals the `daemon_instance_id` `GetHostTooling` answers with, and an online `KnownHostEntry` carries that id as `routing_instance_id`
- [ ] A parent that never answers withholds only its own hosts; the other parents' hosts are offered
- [ ] A parent's earlier answer stays offered until its newer answer arrives; a call past its deadline is that parent's failure
- [ ] A parent that becomes connected after the first listing is asked; a refresh asks every connected parent again
- [ ] Connecting a Coder account on the Accounts screen, and unlocking the vault in the vault prompt, each trigger the refresh
- [ ] A parent's listing failure is shown beside the Host field with its reason; the field still offers every other host
- [ ] The Hosts screen asks an online row's derived hosts and tooling through its `routing_instance_id` when it differs from `instance_id`

## References

### Affected Features (Complete List)

- **Hosts screen**: [hosts-screen.md](../hosts-screen.md) — derived rows are asked through the id the daemon answers on
- **Create-session package**: [create-session-package.md](../create-session-package.md) — the Host field groups derived hosts under the daemon the web addresses and shows a parent's listing failure

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-16-remote-shell-derived-host-identity.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`

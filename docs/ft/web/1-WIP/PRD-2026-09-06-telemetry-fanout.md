# PRD — Hosts screen: live CPU and disk on every host row

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web`
**Stack:** `#hosts-screen` node 2 of 8
**Branch:** `feature/hosts-screen/telemetry-fanout` → base `feature/hosts-screen/host-registry`

## Affected features

| Document | Relationship |
|---|---|
| [`PRD-2026-09-06-host-registry.md`](./PRD-2026-09-06-host-registry.md) | Supplies the rows this node adds telemetry to |
| [`docs/ft/web/host-stats-footer.md`](../host-stats-footer.md) | Defines the existing single-host `StreamHostStats` feed this node fans out |

## Summary

Give every row on the Hosts screen **live** resource telemetry: per-core CPU utilization and free disk,
streaming, one `StreamHostStats` subscription per **online** host.

The daemon already streams exactly this data — but only for the *selected* daemon, into the sessions
drawer footer. `useHostStats` (`packages/tddy-web/src/rpc/useHostStats.ts:38`) resolves its client via
`useDaemonClient(ConnectionService)`, which is the selected host and nothing else. A screen that lists
the whole fleet has to ask every host itself.

## Background

`packages/tddy-web/src/rpc/useHostFanOut.ts` exists for exactly this shape of problem — "anything that
has to show the whole fleet has to ask every daemon itself" — but its `HostReader.read` returns
`Promise<readonly T[]>`. It is **unary-shaped and cannot express a subscription**. Its own doc comment
(`:19-23`) records that `useModelRegistryFanOut` had to be written separately for a composite case.

So this node introduces the streaming counterpart, and it is the first of its kind in the codebase.

## Proposed changes

### What is changing

- **A per-host telemetry hook.** `useHostStats` is generalized to take a host id and resolve its client
  through `useHostClient(ConnectionService, hostId)` (`rpc/connections/registry.tsx:207`) instead of
  `useDaemonClient`. The existing zero-argument call in `HostStatsFooter` keeps working against the
  selected host.
- **A telemetry column on each Hosts row** — per-core CPU bars and free disk, reusing the existing
  `CpuCoresIndicator`, `DiskSpaceIndicator` and `formatDiskFree` (`hostStatsFormat.ts:14`) rather than
  drawing new ones.
- **Subscriptions are opened only for hosts the registry reports online**, and only while the Hosts
  screen is mounted.

### What is staying the same

- **`StreamHostStats` itself is untouched** — no proto change, no daemon change in this node. It is a
  pure web-side fan-out over an existing feed. (Node 3 adds fields to it.)
- **`HostStatsFooter` keeps its current behaviour and appearance.**
- The host registry, the `#/hosts` route and the screen shell are node 1's and are not modified.

## Impact analysis

### Technical

- **N concurrent server streams**, one per online host, each emitting every 5 s. Bounded by
  subscribing only for online hosts and only while the screen is mounted.
- **No new daemon-side stream, so no new teardown obligation.** `packages/tddy-codegen/docs/server-streaming.md`
  requires a `tx.closed()` select only for a handler whose stream can be silent; `stream_host_stats`
  emits unconditionally.
- **A testability gap has to be closed in this node.** `StreamHostStatsRequest` carries only
  `session_token` (`connection.proto:2245-2248`), and `mountWithRpc` hands every host the *same*
  in-memory transport — so a component test cannot currently tell two hosts' subscriptions apart. The
  shared `hostStatsStreamCount()` counter must become a per-host tally.

### User

- Each host row shows live CPU and disk. An offline row shows `—`.
- No existing screen changes.

## Acceptance criteria

- [x] **AC-1** An online host's row shows its per-core CPU bars and free disk, sourced from a live stream.
- [x] **AC-2** The row updates as fresh readings arrive, without a reload.
- [x] **AC-3** Exactly **one** subscription is opened per online host — not one per render.
- [x] **AC-4** An **offline** host's row shows `—` and opens **no** subscription.
- [x] **AC-5** A host whose connection is connecting or errored shows a pending/unavailable state and
      **never fabricated zero values**.
- [x] **AC-6** Leaving the Hosts screen tears every subscription down.
- [x] **AC-7** `HostStatsFooter` is unchanged — still one subscription against the selected host.

## Out of scope for this node

Memory, load average and explicit core count (node 3). Any probe of software installed on a host
(nodes 4–8). Any change to `StreamHostStats`'s proto surface.

## Successor PRs

- `feature/hosts-screen/host-resources` — memory, load average and core count in the telemetry surface.

All seven are pinned by a passing test. AC-3's per-host clause and AC-6's teardown are pinned by
unit tests (`hostTelemetryState.test.ts`, `hostStatsSubscription.test.ts`) rather than by the
component suite, because neither property is observable through the in-memory transport — see the
changeset's red-phase section for the evidence.

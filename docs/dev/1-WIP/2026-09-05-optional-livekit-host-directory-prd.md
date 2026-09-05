# PRD: a host directory that does not require LiveKit

**Stack:** `optional-livekit` — node 2 of 7 (`host-directory`)
**Target PRD on wrap:** [`docs/ft/web/daemon-selector-livekit-rpc.md`](../../ft/web/daemon-selector-livekit-rpc.md)
**Date:** 2026-09-05

## Problem

Node 1 made *talking to* a host transport-neutral. *Knowing which hosts exist* is still LiveKit and
only LiveKit:

- `SelectedDaemonProvider` calls `useCommonRoom(livekitUrl, commonRoom, identity)`, which mints a
  LiveKit token, constructs a `livekit-client` `Room`, and connects it.
- The host list is `daemonHostsFromParticipants(useRoomParticipants(room))` — hosts are literally
  LiveKit participants whose metadata parses as a daemon advertisement (`lib/participantRole.ts`).
- `SelectedDaemonContextValue` publishes `room: Room | null` to the whole subtree.
- With no LiveKit configuration the provider stays `idle` forever, `daemons` is `[]`, and the
  selector offers nothing — **including the daemon that is serving the page**, which the client
  config already names as `daemon_instance_id`.

So a desktop app with no LiveKit settings has no hosts at all, even though it is running one.

## What this PR delivers

A **host directory** assembled from pluggable sources, with the LiveKit common room as one of them,
and `room` removed from the shared context.

### The model

```ts
interface HostDescriptor {
  readonly hostId: string;
  readonly label: string;
  readonly sourceId: string;             // which source contributed it
  readonly reposBasePath?: string;
  readonly maxAttachmentBytes?: number;
}

interface HostDirectorySource {
  readonly id: string;                   // "livekit", later "local-ipc"
  readonly status: ConnectionStatus;
  readonly error: string | null;
  readonly hosts: readonly HostDescriptor[];
}
```

The directory is the merge of every registered source, de-duplicated by `hostId`. Its status is
worst-of over the sources that are expected to be up, so one dead source cannot claim the whole
directory is broken.

### Acceptance criteria

1. `useHostDirectory()` returns the merged host list plus a per-source status map.
2. `LiveKitHostDirectorySource` reproduces today's list exactly: common-room participants whose
   metadata parses as a daemon advertisement, with `reposBasePath` and `maxAttachmentBytes` carried
   through.
3. With **no** `livekitUrl` / `commonRoom` configured, the LiveKit source contributes nothing, does
   not construct a `Room`, does not call `TokenService`, and reports status `idle` — not `error`.
4. In that same case the directory still yields **one** host: the daemon serving the page, from
   `servingInstanceId`. Selecting it works, and every daemon-level screen functions over the
   serving-daemon transport.
5. `room` is gone from `SelectedDaemonContextValue`. Presence consumers read it through an explicit
   capability accessor (`useHostPresence(hostId)`), which returns `null` when the host's connection
   does not advertise `presence`.
6. `roomStatus` / `roomError` on the context are renamed to directory-level status and keep their
   present semantics for the selector chrome (`DaemonSelector`, `connectionChromeStatus`).
7. Host selection, persistence and URL sync (`resolveSelectedDaemonInstanceId`,
   `readStoredSelectedDaemon`, the `host` URL param, the `key={selectedInstanceId}` remount) are
   unchanged in behaviour.
8. A test can drive the whole daemon-mode shell from an in-memory directory source with no
   `livekit-client` import in the tree.

### Non-goals

Session connections, media gating, the IPC source itself, and desktop registration. See the
changeset's `## Boundaries`.

## Why this shape

- **Sources, not a flag.** The desktop app contributes its own host; the browser contributes none of
  its own and relies on the common room. A merge over sources expresses both without a branch.
- **`room` must leave the context.** While it is there, any component can reach LiveKit without
  declaring it needs media or presence — and the capability gating in node 4 would have no seam to
  gate on.
- **The serving host is always known.** `/api/config` already carries `daemon_instance_id`. Treating
  it as a directory entry is what makes "no LiveKit" a working configuration rather than an empty
  screen, and it is the fallback the desktop IPC source later replaces with a richer descriptor.

## Constraints

- **Zero new npm dependencies** (no public npm registry; `bun run local-registry-install`).
- `tddy-web` only — no proto, no daemon change.

## Successor PRs

- `feature/optional-livekit/capability-gating` — media and presence surfaces gated on capability.

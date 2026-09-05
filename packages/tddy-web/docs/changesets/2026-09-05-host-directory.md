# 2026-09-05 — the host list becomes a directory merged from sources

**Type:** Architecture

Node 1 made *talking to* a host transport-neutral. *Knowing which hosts exist* was still LiveKit and
only LiveKit: `SelectedDaemonProvider` called `useCommonRoom`, minted a token, connected a
`livekit-client` `Room`, and the host list was literally
`daemonHostsFromParticipants(useRoomParticipants(room))`. With no common room configured the list was
empty — including of the daemon serving the page, which `/api/config` has always named — so a build
that does not join a room could reach no host at all.

`src/rpc/hostDirectory/` now answers it. `HostDescriptor`, `HostDirectorySource` and `HostDirectory`
are the model; `mergeHostDirectory` de-duplicates by `hostId` with **first source winning**, so
source order is precedence and a desktop build's own account of its machine beats the common room's
advertisement of it. Within a source the order is the source's own, because the LiveKit one already
orders by participant ordering and re-sorting would make the selector jump.

Status is **optimistic** — `connected` if any source is, then `connecting`, then `error`, then
`idle`. One working source is a usable directory, and an unconfigured source reports `idle` so it
never drags the directory into `error`. That single rule is what makes an absent LiveKit
configuration a choice rather than a fault: `error` means every source failed, and only then is a
reason published. A failure on one source while another still names hosts stays that source's, read
off `sources`.

`useLiveKitHostDirectorySource` reproduces the old list exactly and carries `reposBasePath` /
`maxAttachmentBytes` through. `useServingHostDirectorySource` contributes the serving daemon from
`servingInstanceId`. Unconfigured LiveKit reaches `useCommonRoom`'s existing guard, so **no `Room` is
constructed and no `TokenService` call is issued** — the source is `idle`, not `error`.

The optimism has a consequence the code now states in one place: because the serving source is
connected from the first render, a daemon-served page reads `connected` while the common room is
still joining, and the fleet grows when it lands. A surface that is *about* the common room therefore
reads `useHostDirectorySource(LIVEKIT_SOURCE_ID)` rather than the merged status — `DaemonSelector`'s
"Common room unreachable" placeholder and the LiveKit presence screen both do, and wiring either to
the merged status would make its message unreachable on every production page.

`room` is gone from `SelectedDaemonContextValue`; `roomStatus` / `roomError` become
`directoryStatus` / `directoryError`. Presence is now asked for by name —
`useHostPresence(hostId)` returns `null` unless that host's connection advertises the `presence`
capability. The room reaches it through a context owned by `hostDirectory/`, not through
`HostConnection`, whose `Room` stays private on purpose: holding a connection must not let a
component help itself to LiveKit. `LiveKitAppPage`, `RpcPlaygroundAppPage` and `SessionsDrawerScreen`
are migrated onto it. That refusal is the seam capability gating is built on next.

`SelectedDaemonProvider` composes the sources and renders
`HostDirectorySources → HostPresenceRoom → LiveKitConnections → SelectedDaemonScope`, with
`LiveKitConnections` still above the `key={selectedInstanceId}` boundary — a host change reloads the
screens, it does not re-register the wire. Selection, persistence, URL sync and that remount are
behaviourally unchanged. `" (this daemon)"` is now one exported constant in `lib/participantRole.ts`,
which owns the advertisement contract; it had been spelled independently in three places.

**Not here, and deliberately:** no second `ConnectionProvider`. The directory now *names* the serving
daemon but nothing can yet *reach* it, so selecting it on a page whose common room is down resolves
no connection and each screen renders its own "no connection" state. Naming and reaching are separate
concerns and this node owns only the first; the wire follows in the stack. Also absent: session-bound
connections, capability gating of the media and presence surfaces, the IPC source, and desktop
registration.

Three existing specs were re-scoped rather than weakened. With a serving daemon always contributing a
host, preconditions that depended on an empty list became unreachable, so each now mounts the
configuration it was actually about — the common room as sole contributor.

No proto, no Rust, no new npm dependency. Tests: 1002 unit (10 added), 1231 Cypress component across
209 specs. Two of the added specs close gaps where a test could not fail: presence needs a *registered
wire* and a room in scope for the capability gate to be the variable under test, and the serving
source has to be asserted through the real `SelectedDaemonProvider` rather than a hand-built source
literal. Technical [host-directory.md](../host-directory.md), feature
[daemon-selector-livekit-rpc.md](../../../../docs/ft/web/daemon-selector-livekit-rpc.md).

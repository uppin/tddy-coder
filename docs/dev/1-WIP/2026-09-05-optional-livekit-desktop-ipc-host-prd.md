# PRD: reach the local host over IPC, with LiveKit optional

**Stack:** `optional-livekit` — node 7 of 7 (`desktop-ipc-host`)
**Target PRD on wrap:** [`docs/ft/desktop/tddy-desktop-tauri.md`](../../ft/desktop/tddy-desktop-tauri.md)
**Date:** 2026-09-05

## Problem

This is the node the whole stack exists for. Everything is in place and nothing is wired:

- Nodes 1–4 gave `tddy-web` a provider registry, a source-merged host directory, one session
  connection carrying capabilities, and surfaces gated on those capabilities.
- Node 6 gave the desktop app concurrent addressed IPC connections — `Daemon` and
  `Session { session_id }` — with a resolver, a disconnect and per-connection lifecycle.

But the desktop app still registers **no** provider, so it still reaches its own host the only way
`tddy-web` knows how: over LiveKit, from a common room it does not join by default. The result today
is a desktop app that shows no hosts, or one it cannot talk to.

And the reverse case has to keep working: a desktop app that *is* configured for LiveKit must still
see and use other hosts through it — while its **own** host stays on IPC, which is both faster and
available with no configuration at all.

## What this PR delivers

The desktop build registers its own connection provider and directory source. Its own host is reached
over IPC; every other host, when LiveKit is configured, is reached over LiveKit; and with no LiveKit
configuration the app is fully functional on one host.

### Acceptance criteria

1. The desktop build registers an `IpcConnectionProvider` and a `LocalHostDirectorySource` at
   startup. `tddy-web` imports neither — they arrive through node 1's registry and node 2's source
   list, so the browser bundle is unchanged.
2. Selecting the local host yields a `HostConnection` over the `Daemon`-targeted IPC connection,
   advertising `{"rpc"}`.
3. `hostConnection.openSession(sessionId, hint)` on the local host opens a
   `Session { session_id }` IPC connection — a **separate, concurrent** connection, created the same
   way a LiveKit room and participant are created for a session, and released by `close()` when the
   session is detached.
4. Attaching several sessions at once opens several IPC connections at once; detaching one does not
   disturb the others.
5. **No room, participant, token or LiveKit identity appears on the IPC path** — not in the
   provider, not in the directory source, not in the hint the provider reads.
6. With **no** LiveKit configuration (`livekitUrl` / `commonRoom` empty in the payload
   `DaemonConfigService.GetClientConfig` serves), the desktop app: joins no room, mints no token,
   constructs no `Room`, shows exactly its own host, and has every media and presence surface absent
   rather than broken.
7. With LiveKit configured, the directory shows the local host **and** the common room's peers.
   Selecting a peer reaches it over LiveKit with `{"rpc", "media", "presence"}`; selecting the local
   host still reaches it over IPC. Both are usable in one session of the app, without a reload.
8. A LiveKit connection failure degrades the peers, not the local host: the local host stays
   selectable and fully functional while the directory reports the LiveKit source's error.
9. In a **browser**, choosing the desktop machine's host still uses LiveKit — the IPC override does
   not exist there, and nothing about the browser path changes.
10. The `tddy-desktop` e2e (`e2e/signIn.e2e.ts`, `wdio.conf.ts`) passes, and a `./desktop-dev` run is
    reported showing sessions attaching over IPC.

### Non-goals

Media over IPC, a second LiveKit-for-media connection on the desktop's own host, and any change to
nodes 1–6's surfaces. See the changeset's `## Boundaries`.

## Why this shape

- **A source plus a provider, registered by the host build.** That is the whole point of nodes 1
  and 2: the desktop contributes what it knows, and `tddy-web` stays ignorant of which wire it got.
  No `isDesktop` branch appears anywhere.
- **LiveKit stays available, and stays optional.** The user's requirement is both directions: the
  desktop app must work with no LiveKit at all, *and* must use LiveKit for peers when configured.
  Because reachability is per host and per connection, both fall out of the same mechanism rather
  than needing a mode switch.
- **The local host over IPC even when LiveKit is available.** It is in-process: a round trip through
  a media server to reach a daemon in the same binary is pure latency. Provider precedence — a
  registered IPC provider wins for the host it claims — expresses that without a preference setting.
- **`{"rpc"}` only, honestly.** The daemon serving the desktop *could* publish media into a LiveKit
  room, but that would mean the desktop's own host silently requiring the thing this stack made
  optional. Absent surfaces are the correct answer; a later node can revisit if the demand is real.

## Constraints

- **Zero new npm dependencies and no new Rust dependencies** (no public npm registry;
  `bun run local-registry-install`).
- **`tddy-desktop` is not in the CI gate** (`docs/dev/guides/ci.md`) and neither is Cypress e2e. Both
  desktop paths — LiveKit configured and not — need a reported local run.
- No proto change: `DaemonConfigService.GetClientConfig` already carries `livekitUrl`, `commonRoom`
  and `daemonInstanceId`, and the desktop already reads it over IPC.

## Successor PRs

None — this is the stack's top node.

# LiveKit as a satellite integration - PRD

**Date**: 2026-10-16
**PRD Type**: Bug Fix + Technical Improvement (architecture)

## Affected Features

- **Primary Feature**: [Session room](../session-room.md) — `ConnectSession` stops opening the
  session room; a new plugin-owned `OpenSessionRoom` takes over the "open it or fail loudly"
  contract that `ConnectSession` carries today.
- **Related Feature**: [Session worktree sync](../session-worktree-sync.md) — `tddy-session-sync`
  asks `OpenSessionRoom` instead of `ConnectSession` before joining.
- **Related Feature**: [Terminal sessions](../terminal-sessions.md) — `tddy-tools pty-relay
  --livekit-url` asks `OpenSessionRoom` instead of `ConnectSession` before joining; attaching to a
  local session's terminal never waits on LiveKit.
- **Related Feature**: [Daemon settings](../daemon-settings.md) — the daemon reports each optional
  integration's live state (first: LiveKit) through `DaemonConfigService`.
- **Related Feature**: [Tddy desktop app](../../desktop/tddy-desktop-tauri.md) — a desktop with
  LiveKit configured and unreachable keeps working on its own sessions.

## Summary

LiveKit is a **satellite**: when it is configured and reachable, a daemon gains remote reach —
remote clients, peer daemons, session mirrors, split placements, screen sharing. When it is absent,
switched off, misconfigured or unreachable, **everything local works exactly as if it were absent**,
and the operator can see which of those it is.

Architecturally, LiveKit becomes a **plugin**: the daemon's core defines **extension ports**, the
LiveKit crates implement them, and only the crates that build binaries bring the two together.
Library crates do not depend on `livekit`, `livekit-api`, `livekit-protocol`, `tddy-livekit*` or
`tddy-daemon-livekit`.

Delivered as a **PR stack**. Node 1 (this PRD's first deliverable) defines the ports, fixes the
reported defect through them, exposes integration status, and installs a dependency ratchet that
every later node tightens.

## Background

**Reported 2026-10-16.** On Tddy Desktop, an operator saved a LiveKit block pointing at a server
that does not answer. Two sessions started normally — agents running, worktrees created — but
neither terminal appeared; the pane read "Select Resume to reconnect". `ConnectSession` for a
`claude-cli` session opens the session room before it answers, the room open failed against the
unreachable server, and the local client's attach failed with it — although the desktop reaches its
own sessions over IPC and never uses the room.

**Why this keeps recurring.** The `#optional-livekit` stack (#437–#451) made the client work without
LiveKit, made LiveKit switchable, and made session *start* stop waiting on it. Its own evaluation
recorded that "is LiveKit configured?" is decided in at least six places, and that #451 existed only
because one of those re-derivations put LiveKit back on a local path. This defect is the same shape
on `ConnectSession`. Each fix so far has been local; nothing in the structure stops the next one.
LiveKit reaches 37 workspace crates through normal dependencies — most of them through the daemon
kernel's `peer_forwarding` module — so any core code path can reach the SDK, and does.

**Why #451 kept `ConnectSession` loud, and why that stays true.** `tddy-session-sync`,
`tddy-tools pty-relay --livekit-url` and remote web clients call `ConnectSession` *because* they need
the room, and turning a room failure into a silent success would hand them a room nobody hosts. This
PRD keeps that guarantee for those callers and moves it to where it belongs: an RPC the LiveKit
plugin owns.

## Proposed Changes

### What's Changing

**1. Local attach never depends on LiveKit.**
- `ConnectSession` does local work only. It reports that the session was **reached** through a
  core extension port, passing the transport the request arrived on (host-stamped
  `RequestTransport`, which a caller cannot choose).
- The LiveKit plugin's implementation of that port decides:
  - a caller that **arrived over LiveKit** (a remote web client) needs the room and its terminal
    bridge now — the plugin opens them before `ConnectSession` answers, and a failure fails the
    call, naming the room;
  - any other caller (in-process IPC, HTTP, gRPC, Unix socket) is local — the plugin opens the room
    **in the background** so the session becomes remotely reachable, and records the outcome in the
    integration status. `ConnectSession` answers immediately either way.
- With no LiveKit plugin wired, or with LiveKit unconfigured, reaching a session does nothing.

**2. "Open the room or fail" becomes a plugin-owned RPC.**
- New `SessionRoomService.OpenSessionRoom(session_token, session_id)` → `{ livekit_room,
  livekit_url, livekit_server_identity }`, registered by the LiveKit plugin, absent when it is not
  wired. It opens (or reuses) the room and the terminal bridge, and **fails loudly** when it cannot.
- `tddy-session-sync` and `tddy-tools pty-relay --livekit-url` call it instead of `ConnectSession`
  before joining.

**3. Integration status is observable.**
- The daemon keeps one **integration status board**; each plugin reports its own state into it.
- States: `disabled` (switched off or not configured), `misconfigured` (enabled with a field
  missing — names the field), `connecting`, `connected`, `unreachable` (with the last error and
  when it happened).
- Read through `DaemonConfigService.GetIntegrationStatus` — served over HTTP/IPC, so readable when
  LiveKit is the thing that is down. This is the "resolve availability once" seam the
  `#optional-livekit` evaluation asked for.
- Node 2 shows it in the web settings screen; node 1 delivers the read.

**4. Extension ports and the plugin boundary.**
- New crate **`tddy-daemon-extensions`**: port traits and their named null objects
  (`NoSessionReach`, …), the integration status board. Depends on no LiveKit crate, transitively.
- Core crates depend on the ports; `tddy-daemon-livekit` implements them; `tddy-daemon` (and
  `tddy-desktop` through it) wires them.
- Each later stack node moves one cluster of LiveKit consumers behind a port (see Implementation
  Plan).

**5. A dependency ratchet.**
- A workspace test lists which crates may name a LiveKit crate directly: the plugin crates and the
  binary/wiring crates. Every other crate that does so today is listed as a **known offender**.
- Adding a new offender fails. Removing one without deleting it from the list also fails, so the
  list only shrinks, and it always matches the tree. The last stack node deletes the list.
- `tddy-daemon-extensions` additionally may not reach any LiveKit crate **transitively**.

### What's Staying the Same

- Session start, resume, signal and delete: unchanged (already LiveKit-free on co-located
  placements).
- Split-placement start still opens its room eagerly and fails loudly — LiveKit is that
  placement's transport, not an addition to it.
- `AttachSessionAgent` and seeded-clone claims still require a room and refuse without LiveKit.
- `ConnectSession`'s reply shape: empty coordinates for `claude-cli`, `cursor-cli`, `workspace`;
  room coordinates for tool sessions.
- The web client (`tddy-web`) is unchanged in node 1: it already ignores LiveKit coordinates for a
  local host, and a remote web client still gets its room before `ConnectSession` answers.
- Config shape (`livekit:` block) and the runtime reconfiguration supervisor.

## Impact Analysis

### Technical Impact

- New crate `tddy-daemon-extensions`; new proto service `SessionRoomService` and RPC
  `DaemonConfigService.GetIntegrationStatus` in `tddy-service`.
- `tddy-agent-launch`: `ConnectSession` loses its `ensure_session_room` call and gains a
  `SessionReach` port.
- `tddy-daemon-livekit`: implements `SessionReach`, owns `OpenSessionRoom`, reports status from the
  common-room discovery loop and the room opener.
- `tddy-session-lifecycle`: builder for the reach port; `ensure_session_room` remains for the
  agent-roster paths.
- `tddy-session-sync`, `tddy-terminal-rpc`: switch to `OpenSessionRoom`.
- `tddy-daemon`: wires plugin → ports; registers the plugin's RPC entry.
- `tddy-testing-commons`: one shared dependency-graph helper (closes the duplicated-harness TODO).
- Background room opens hold the per-session opening lock; an `OpenSessionRoom` that arrives while
  one is in flight waits for it rather than racing it (unchanged single-flight semantics).

### User Impact

- A desktop or local web operator with LiveKit configured but unreachable sees their sessions'
  terminals immediately.
- Remote reach fails where it is asked for, loudly, and the operator can read why from the status.
- **Breaking for out-of-tree callers of the old contract**: a client that relied on `ConnectSession`
  to open a room for a `claude-cli`/`cursor-cli` session from a non-LiveKit transport must call
  `OpenSessionRoom`. The two in-tree callers are migrated in node 1. A session-sync or pty-relay
  binary older than the daemon would join a room that may not be open yet and time out waiting for
  the daemon — mixed-version installs need both upgraded together.

## Implementation Plan (PR stack)

1. **Extension ports, local `ConnectSession`, `OpenSessionRoom`, integration status read,
   dependency ratchet.** *(this PRD's first changeset)*
2. **Integration status in the UI** — settings LiveKit block and host chrome show the board.
3. **Peer forwarding behind a port** — the kernel's `peer_forwarding` moves to the plugin behind a
   transport-neutral `PeerForwarder`; the SDK leaves `tddy-daemon-kernel` (closes
   `packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md`).
4. **LiveKit-free modules leave the plugin** — `placement`, `os_user_resolution`, `session_git` and
   the identity re-exports move to core; drop unused `livekit` edges (`tddy-daemon-rpc`,
   `tddy-session-lifecycle`).
5. **Session-room registry behind a port** — `tddy-session-activity`, `tddy-telegram-control`,
   `tddy-agent-launch`, `tddy-session-lifecycle` see a `SessionRooms` port.
6. **Token minting behind a port** — `tddy-daemon-auth`, `tddy-screen-sharing`,
   `tddy-session-split`.
7. **Peer-forwarding consumers** — `tddy-daemon-rpc`, `tddy-session-files`, `tddy-host-service`,
   `tddy-agent-launch` route through `PeerForwarder`.
8. **Agent clones and split transport** — `tddy-session-agents`, `tddy-session-split`; lifecycle's
   plugin re-exports removed.
9. **Terminal bridge as a plugin** — `tddy-cli-sessions`' LiveKit bridge implements a terminal
   exposure port.
10. **Client libraries** — `tddy-coder`, `tddy-session-sync`, `tddy-remote-git-repo`,
    `tddy-terminal-rpc`, `tddy-session-tool-client`, `tddy-discovery`: LiveKit only in their
    binaries' wiring.
11. **Ratchet closes** — the known-offender list is empty and deleted; docs describe the plugin
    boundary.

Order 3→4 is chosen so the largest closure win lands early; nodes 5–10 are independent of each other
apart from shared ports and may be re-ordered at stack planning.

## Success Criteria

- With LiveKit configured and unreachable, `ConnectSession` for a local `claude-cli` session
  returns within the time it takes with LiveKit unconfigured, and dials LiveKit zero times on the
  caller's path.
- With LiveKit configured and unreachable, `OpenSessionRoom` fails naming the session's room.
- With LiveKit reachable, a session connected locally becomes drivable over LiveKit without a further
  call; a remote client arriving over LiveKit can drive the terminal as soon as `ConnectSession`
  answers.
- `GetIntegrationStatus` reports `unreachable` with the server's error while LiveKit is down,
  `connected` once it answers, `disabled` with no block, `misconfigured` naming the missing field.
- `tddy-daemon-extensions` reaches no LiveKit crate; the ratchet's offender list equals the tree.
- At stack end: no library crate outside the plugin set depends on a LiveKit crate.

## Out of Scope

- `tddy-web` LiveKit client code (already behind its `HostConnection`/`SessionConnection` seam).
- Hot-applying an `api_key`/`api_secret`-only edit to the running common room (being fixed
  separately in the `fix-livekit-secret` session).
- A client-side deadline for LiveKit RPC calls
  ([2026-08-14 TODO](../../../dev/todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md)).
- A cargo `livekit` feature on the daemon binary (possible once the ratchet closes; not required).

# Extension ports

How an optional integration hooks into the daemon without the daemon depending on it. LiveKit is the
first integration; the shape is the same for any that follows.

The daemon's core crates depend on `tddy-daemon-extensions`. An integration's crates implement its
ports. Only the crates that build a binary (`tddy-daemon`, and the embedders) bring the two together.
A daemon with nothing wired gets each port's **null object**, which records the decision "no
integration" instead of a `None` that could equally mean "wiring forgotten".

The crate depends on `tddy-rpc` and `async-trait` and on nothing that reaches an integration.

## The session-reach port (`session_reach`)

A session is **reached** when something connects to it. The core reports every reach, with the
transport the request arrived on, and the integration decides what that costs the caller.

| Item | Role |
|---|---|
| `ReachedSession` | The session at the moment it was reached: `session_id`, `session_dir`, `worktree_root` (the checkout the session's agent works in, on this host) and `arrived_over` (`tddy_rpc::RequestTransport`, stamped by the receiving host; a caller cannot choose it). |
| `SessionReach` | `session_reached(session, host)`. An `Err` fails the request that reached the session, so an implementation returns one only when that caller cannot be served without what failed. |
| `ReachableSessions` | The session host as an extension sees it, implemented once by the session host: `resolve(session_token, session_id)` authenticates and resolves a session for an extension's own RPC (the result's `arrived_over` is `Direct`, because the extension's RPC, not the core, received the request); `session_services()` is the RPC surface a participant outside this host reaches the session through; `expose_terminal(session_id)` makes the session's terminal drivable from outside this host (a session with no terminal is not a failure). |
| `NoSessionReach` | No extension wired: a session is reachable on this host and nowhere else. `session_reached` returns `Ok` without consulting the host. |

`ConnectSession` reports a reach only for a session whose type this daemon facilitates and which
records a checkout; the rule lives in one function, `facilitated_checkout`, in
`tddy-agent-launch::session_coordinate_handlers`, and the same function guards an integration's own
RPC through `ReachableSessions::resolve`. See
[LiveKit session reach](../../tddy-daemon-livekit/docs/livekit-service.md#livekitsessionreach--opening-the-room-for-a-reached-session)
for the implementation of the port.

## The integration status board (`integration_status`)

Each integration is handed an `IntegrationReporter` at wiring time and reports into the daemon's one
`IntegrationStatusBoard`; `DaemonConfigService.GetIntegrationStatus` reads the board. Nothing
re-derives "is it configured, is it up" from configuration on its own.

| Item | Role |
|---|---|
| `IntegrationState` | `Disabled`, `Misconfigured { missing }` (field names, in configuration order), `Connecting`, `Connected`, `Unreachable { error }` (the last failure). |
| `IntegrationStatus` | One entry: `id`, `label`, `state`, `since`. |
| `IntegrationStatusBoard` | `new()` (shared, `Arc`), `reporter(id, label)`, `snapshot()` (every entry, ordered by id). |
| `IntegrationReporter` | Clonable handle on one entry; `report(state)`. |

Semantics:

- `reporter` registers the integration as `Disabled` and returns its reporter. Registering an `id`
  that is already on the board keeps the existing entry, so a second reporter never resets what the
  first has reported.
- `since` is when the integration entered its current **kind** of state. A report of the state it is
  already in leaves `since` alone, and so does a report of the same kind with different detail: an
  `Unreachable` whose error text varies between retries is one outage, not several.
- All reporters of one `id` write the same entry; the last writer wins.
- A reporter's entry is registered when the reporter is made, so `report` cannot miss it.

## The session-rooms port (`session_rooms`)

Code that only needs to close a session's room, or ask whether this daemon hosts one, holds a
`SessionRooms` instead of naming the integration's registry.

| Item | Role |
|---|---|
| `SessionRooms` | `hosts(session_id)`: whether this daemon currently hosts the session's room. `close(session_id)`: stop hosting it, if one is hosted. `close_for_worktree(worktree)`: stop hosting every room measuring the checkout, before the checkout is removed. |
| `NoSessionRooms` | No integration hosting rooms: nothing is hosted, and closing is a no-op. |

`SessionRoomRegistry` implements the port in `tddy-daemon-livekit` (`hosts` is the registry's
`contains`). The daemon hands the one shared registry to the consumers as `Arc<dyn SessionRooms>`:
`tddy-session-activity::session_deletion::close_session_room`, the Telegram control harness
(`with_session_rooms`, defaulting to `NoSessionRooms`) and `tddy-worktree-service`, which re-exports
the port as `WorktreeRoomCloser`. Opening a room is not on this port.

## The satellite rule and its ratchet

**Nothing in this crate may reach a LiveKit crate**, transitively, so a core crate that depends on
the ports gains no LiveKit dependency through them. `tests/livekit_is_a_satellite.rs` holds the rule
over the workspace graph read by
[`WorkspaceGraph`](../../tddy-testing-commons/docs/dependency-graph.md):

| Test | Holds |
|---|---|
| `the_extension_ports_reach_the_crates_they_are_built_on` | The walk from this crate sees `tddy-rpc`, so the next test is not vacuous. |
| `the_extension_ports_reach_no_livekit_crate` | `reaches("tddy-daemon-extensions")` shares nothing with `LIVEKIT_FAMILY`. |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` | Every workspace crate that names a LiveKit crate directly is on `PLUGINS`, `WIRING` or `KNOWN_OFFENDERS`. |
| `every_known_offender_still_names_a_livekit_crate` | A crate that stopped naming one must be struck from `KNOWN_OFFENDERS`, so the list says what the tree does. |

The lists, in the test file:

- `LIVEKIT_FAMILY`: `livekit`, `livekit-api`, `livekit-protocol`, `tddy-livekit`,
  `tddy-livekit-testkit`, `tddy-daemon-livekit`, `tddy-livekit-screen-capture`, `tddy-screenshare`.
- `PLUGINS` (LiveKit's own workspace crates, which implement the ports): `tddy-livekit`,
  `tddy-livekit-testkit`, `tddy-daemon-livekit`, `tddy-livekit-screen-capture`, `tddy-screenshare`.
- `WIRING` (binaries' composition roots and the end-to-end harness): `tddy-daemon`, `tddy-rdp`,
  `tddy-vnc`, `tddy-e2e`.
- `KNOWN_OFFENDERS` (library crates that still name a LiveKit crate): `tddy-agent-launch`,
  `tddy-cli-sessions`, `tddy-coder`, `tddy-daemon-auth`, `tddy-daemon-kernel`, `tddy-daemon-rpc`,
  `tddy-host-service`, `tddy-remote-git-repo`, `tddy-screen-sharing`,
  `tddy-session-agents`, `tddy-session-files`, `tddy-session-lifecycle`, `tddy-session-split`,
  `tddy-session-sync`, `tddy-session-tool-client`, `tddy-terminal-rpc`.

"Names" is a `[dependencies]` or `[build-dependencies]` entry, optional ones included;
`[dev-dependencies]` are not counted, because a test may stand up a real LiveKit server. The
transitive walk is applied to this crate only; for every other crate the ratchet holds the direct
edge, which is what each cluster that moves behind a port can remove.

**The list only shrinks.** A crate that starts naming a LiveKit crate and is on no list fails the
second test; an offender that stops fails the third until it is struck off. Adding a crate to
`KNOWN_OFFENDERS` to make a build pass is the one edit the ratchet exists to make conspicuous.

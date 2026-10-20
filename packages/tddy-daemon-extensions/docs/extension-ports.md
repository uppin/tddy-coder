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

## The terminal-exposure port (`terminal_exposure`)

Serving a session's PTY to participants outside this host goes through a `TerminalExposure`. The CLI
session manager hands the port the terminal's RPC service, a room-scoped address and the session's
metadata block; the integration decides how the terminal becomes reachable.

| Item | Role |
|---|---|
| `TerminalAddress` | `room` and `identity`: where the terminal is served. Credentials are not part of it; only the integration needs them. |
| `TerminalExposure` | `expose(at, terminal, metadata_json)`: serve `terminal` (an `Arc<dyn tddy_rpc::RpcService>`) as `at.identity` in `at.room`, advertising `metadata_json`. |
| `ExposedTerminal` | A running exposure. `is_finished()` is true once the room or the terminal ended, which is what lets the manager expose again. |
| `NoTerminalExposure` | Nothing wired: `expose` returns a failed precondition, which callers treat as "not exposed". |

`CliSessionManager::ensure_exposed_terminal(session_id, at, exposure)` keeps the per-session record
and is single-flight: the registry's write lock is held across `expose`, so two reaches at the same
moment start one exposure, not two that would evict each other under one identity. It answers
`Ok(true)` when an exposure is running, `Ok(false)` when the session recorded no terminal, its PTY is
gone, or the exposure answered with a failed precondition, and an error for any other refusal.
`pty_terminal_service(handle)` builds the RPC service from a PTY handle.

`LiveKitTerminalExposure` implements the port in `tddy-daemon-livekit`. The daemon builds it once
from the `livekit:` block, or the null object without credentials, and hands it to the session host
(`with_terminal_exposure`) and to the Telegram harness, whose start announces its terminal's room at
once.

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

## The room-tokens port (`room_tokens`)

Code that needs a LiveKit-style room access token for a room and identity holds a `RoomTokenMinter`
instead of building a token generator from the `livekit:` block's credentials.

| Item | Role |
|---|---|
| `RoomTokenMinter` | `mint(room, identity, ttl)`: a signed room access token, or a `Status`. Performs **no authorization**: hand it only to code that has already decided the caller may have the token. |
| `NoRoomTokens` | No integration: every mint is a failed precondition. |

`tddy_daemon_livekit::room_tokens::room_token_minter(Option<&LiveKitConfig>)` builds the daemon's one
minter from the `livekit:` block: a real one when `url`, `api_key` and `api_secret` are all set;
otherwise one whose every mint fails naming the missing fields (`NoRoomTokens` when there is no block).
The runtime builds it once and hands it to `tddy-daemon-auth` (`build_auth_entries*`,
`build_token_service_entry`, `LiveKitTokenServiceImpl`) and to `tddy-screen-sharing`
(`with_room_tokens`). `MinterTokenProvider` adapts it onto `tddy_service::TokenProvider`, the token
service's own port, which keeps its fixed TTL and string errors.

## The satellite rule and its ratchet

**Nothing in this crate may reach a LiveKit crate**, transitively, so a core crate that depends on
the ports gains no LiveKit dependency through them. `tests/livekit_is_a_satellite.rs` holds the rule
over the workspace graph read by
[`WorkspaceGraph`](../../tddy-testing-commons/docs/dependency-graph.md):

| Test | Holds |
|---|---|
| `the_extension_ports_reach_the_crates_they_are_built_on` | The walk from this crate sees `tddy-rpc`, so the next test is not vacuous. |
| `the_extension_ports_reach_no_livekit_crate` | `reaches("tddy-daemon-extensions")` shares nothing with `LIVEKIT_FAMILY`. |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` | Every workspace crate that names a LiveKit crate directly is on `PLUGINS`, `WIRING`, `WIRING_WITH_A_LIBRARY` or `KNOWN_OFFENDERS`. |
| `every_known_offender_still_names_a_livekit_crate` | A crate that stopped naming one must be struck from `KNOWN_OFFENDERS`, so the list says what the tree does. |
| `a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary` | The sources of every `WIRING_WITH_A_LIBRARY` crate, apart from `src/main.rs`, contain no path into `livekit`, `livekit_api` or `livekit_protocol`. |

The lists, in the test file:

- `LIVEKIT_FAMILY`: `livekit`, `livekit-api`, `livekit-protocol`, `tddy-livekit`,
  `tddy-livekit-testkit`, `tddy-daemon-livekit`, `tddy-livekit-screen-capture`, `tddy-screenshare`.
- `PLUGINS` (LiveKit's own workspace crates, which implement the ports): `tddy-livekit`,
  `tddy-livekit-testkit`, `tddy-daemon-livekit`, `tddy-livekit-screen-capture`, `tddy-screenshare`.
- `WIRING` (binaries' composition roots and the end-to-end harness): `tddy-daemon`,
  `tddy-coder-cli`, `tddy-rdp`, `tddy-vnc`, `tddy-e2e`. `tddy-coder-cli` builds the `tddy-coder`
  binary and wires `LiveKitParticipantHost` into the library
  ([participant host](../../tddy-coder/docs/participant-host.md)).
- `WIRING_WITH_A_LIBRARY` (crates that ship a library *and* a binary wiring LiveKit into it):
  `tddy-remote-git-repo`, `tddy-session-sync`, `tddy-tools`. Cargo has no binary-only dependency, so
  the edge appears on the crate; the shape test above holds the library sources clean, which keeps the
  allowance to the binary's wiring. These libraries join rooms through
  [`tddy-room-client`](../../tddy-room-client/docs/room-client.md).
- `KNOWN_OFFENDERS` (library crates that still name a LiveKit crate): `tddy-agent-launch`,
  `tddy-session-agents`, `tddy-session-lifecycle`, `tddy-session-split`. `tddy-daemon-auth`,
  `tddy-daemon-rpc` and `tddy-session-files` left it once their peer forwards went through
  `tddy_daemon_kernel::peer_calls` and the OAuth tunnel moved into the plugin.

"Names" is a `[dependencies]` or `[build-dependencies]` entry, optional ones included;
`[dev-dependencies]` are not counted, because a test may stand up a real LiveKit server. The
transitive walk is applied to this crate only; for every other crate the ratchet holds the direct
edge, which is what each cluster that moves behind a port can remove.

**The list only shrinks.** A crate that starts naming a LiveKit crate and is on no list fails the
second test; an offender that stops fails the third until it is struck off. Adding a crate to
`KNOWN_OFFENDERS` to make a build pass is the one edit the ratchet exists to make conspicuous.

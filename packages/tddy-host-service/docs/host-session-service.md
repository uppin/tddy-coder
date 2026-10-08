# HostSessionService (tddy-host-service)

The daemon-hosted service a spawned `tddy-coder` tool session reaches over its **OS user's host-session
socket**. A tool session serves its own toolcall socket, so its agent's `spawn_conversation` and
`github-token` requests land on the *coder's* listener, which can neither create a worktree nor read the
owner's vault. The coder relays them to the daemon as a reverse RPC over one unix socket per OS user.

Code: `src/host_session_service.rs`. Where the socket is bound and who registers sessions:
[session identity](../../tddy-session-lifecycle/docs/session-identity.md).

## Wire

JSON over `tddy-stdio` as `tddy.host.HostSessionService`, with the service and method names
(`HOST_SESSION_SERVICE`, `SPAWN_CONVERSATION_METHOD`, `GITHUB_TOKEN_METHOD`) defined in
`tddy-toolcall` so the coder-side client and this crate agree without depending on each other. It is
not a `.proto` service: there is nothing to regenerate, and nothing in the `unbundle_service_split`
closed-world RPC list.

| Method | Answers from | Returns |
|---|---|---|
| `SpawnConversation` | the session's conversation handler (grill-me only) | the spawned conversation |
| `GithubToken` | the session's `GithubCredentialHandler` | the pinned account's token, or the refusal verbatim |

A host that predates a method answers `unimplemented`, so a new method is additive. Every request
carries `session_id`; one that names no session is refused.

## Registry

`HostSessionRegistry` maps session id to `RegisteredSession { os_user, conversation_spawn_handler,
github_credential_handler }` and, per registration, the pid of the process it was last started as.

- `register` replaces an entry (a resume registers again with the refreshed assignments and token).
- `unregister` drops it (session deletion).
- `attach_process` records the pid after the spawn; `unregister_stopped(session_id, pid)` removes the
  entry **only while that pid is still the registered one**, so an old process stopping late never
  removes a newer registration. A registration with no process is never removed by a stop report.

## Who is answered

`HostSessionService::new(socket_owner, registry)` serves one socket. For each request it:

1. looks the id up in the registry;
2. refuses an unknown or deleted id with `not_found` (the message does not suggest resuming);
3. refuses an id registered for a different OS user than the socket's owner with `permission_denied`,
   naming neither user and carrying no token;
4. otherwise answers from **that session's** handlers, so session A's token is never returned for
   session B's id.

**Trust.** The socket's filesystem permissions are the authentication boundary and `session_id` is a
label, not a secret. Any process running as the socket's OS user can ask for the token of any of that
user's registered sessions. A hostile process of the same OS user is not defended against.

## Sessions that exist but are not registered

`with_session_probe` gives the service a probe that is asked whether a session exists **under the
socket owner's sessions base** (id validated as one path segment). An unregistered session that exists
on disk gets `failed_precondition`:
`this session was started before the daemon restarted; resume it to re-enable GitHub tools`. The probe
is asked about the socket's owner only, so a request can never learn that another OS user's session
exists.

## Tests

`host_session_service` unit tests: the id is required, an unknown method is `unimplemented`, wrong
owner and unknown session refusals, one OS user's socket cannot fetch another's session token, the
stop report (`a_stopped_process_unregisters_its_session_only_while_it_is_still_the_registered_one`,
`a_session_registered_with_no_process_is_not_removed_by_a_stopped_report`) and the existence probe.
The end-to-end behaviour is in `tddy-session-lifecycle/tests/tool_session_host_socket.rs`.

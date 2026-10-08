# Reaching the daemon from a tool session

A `tddy-coder` tool session serves its own toolcall socket for its agent's `tddy-tools`. Two requests
cannot be answered there, because they need daemon-owned work: `spawn_conversation` (a worktree) and
`github-token` (the owner's vault). The coder relays them to the daemon over **the host-session
socket** of its OS user ([host-session service](../../tddy-host-service/docs/host-session-service.md),
[where it is bound](../../tddy-session-lifecycle/docs/session-identity.md#tool-sessions-the-token-over-the-per-os-user-host-session-socket)).

Code: `src/tool_host_wiring.rs`, `src/conversation_spawn_relay.rs`, and the wiring call in `run.rs`.

## Wiring rule

`ToolHostHandlers::from_flags(host_session_socket, session_id)` binds the forwarding handlers
(`DaemonRelayConversationSpawnHandler`, `DaemonRelayGithubCredential`) on the coder's toolcall listener
(`start_toolcall_listener_with_handlers`) **exactly when both** `--host-session-socket` and the
session's id are present. The id is the coder's own (`--session-id`, or `--resume-from`, which `run.rs`
copies into `args.session_id`), so there is no flag for it. With no socket there is no handler, and the
agent's request is refused as having *no credential handler*: there is no fallback. The flag is accepted
for every recipe; nothing keys off its presence to mean grill-me.

## `HostSessionClient`

One client per session: `HostSessionClient::new(socket, session_id)`. Every request names the session.

- It connects on first use and **again after the connection is lost** (a daemon restart that rebinds
  the socket).
- It repeats only an idempotent request (the token request), **never a spawn**.
- The host's status text and refusals are relayed verbatim. An unreachable socket is an error that
  names the socket, with the hint to resume the session if the daemon restarted since it started.

## Prompt flag

The same fact sets `github_pr_tools_available`: `tool_host.github_pr_tools_available()` is true exactly
when a credential handler was bound, and `run.rs` passes it to
`presenter.set_github_pr_tools_available`, which seeds
`tddy_workflow::context_keys::GITHUB_PR_TOOLS_AVAILABLE_KEY` into every run's context. The recipes' hooks
read it ([workflow-recipes](../../tddy-workflow-recipes/docs/github-on-demand.md#prompt-awareness)).

## Tests

`conversation_spawn_relay` and `tool_host_wiring` unit tests: the request names the session, token and
refusal verbatim, an unreachable socket is named, reconnect after the host restarts and rebinds, a spawn
is never repeated, and the flag is true exactly when a handler is bound. The end-to-end path (real coder
listener, forwarding handler, real user socket, registry, account token) is
`tddy-session-lifecycle/tests/tool_session_host_socket.rs`.

# `code_navigation.CodeNavigationService`

Go-to-definition, references and hover for the web's [session code pane](../../../docs/ft/web/session-code-pane.md),
answered by the warm code-intelligence index. The service owns no index: it authorises a request,
then forwards it. `tddy-daemon` wires it; the handler itself is
[`tddy-daemon-rpc`'s `CodeNavigationServiceImpl`](../../tddy-daemon-rpc/docs/architecture.md#code-navigation).

| RPC | Purpose |
|-----|---------|
| `Definition` | Where the symbol at a position of a worktree file is defined |
| `References` | Every reference to that symbol, its declaration included |
| `Hover` | The language server's hover markdown for it; unset when there is none |
| `WatchCodeIndex` | A session's code-index warm-up: latest progress, then each change, ending after `ready` or `error` |

The wire contract is [`tddy-service/proto/code_navigation.proto`](../../tddy-service/docs/code-navigation-proto.md).
`runtime.rs` builds `CodeNavigationServiceImpl` from the daemon's `WorktreeServiceImpl` and, when an
`index_daemon:` section exists, its `IndexDaemonRegistry` — handed over as the handler's
`IndexChannelSource` port, which `index_daemon/registry.rs` implements — and registers the entry
beside the worktree service it borrows. The daemon holds no RPC method of its own for this service.

When an `index_daemon:` section exists, `runtime.rs` also builds one `SessionIndexProgress` holder,
hands it to the service (`with_index_progress`), and installs an `IndexWarmupObserver` on the session
host (`DaemonSessionHost::with_worktree_observer`) that records into the same holder. The index-daemon
block therefore precedes the session host's construction in `build`, so the observer can hold the
registry. Without the section nothing is installed: no session is warmed and `WatchCodeIndex` ends
empty.

## The path every request takes

1. **Authorise exactly as `WorktreeService` does.** The service holds the daemon's
   `WorktreeServiceImpl` and calls its `resolve_listed_worktree` (session token → GitHub user → mapped
   OS user → the project's main repo on this host → the `worktree_path` must appear in that repo's
   `git worktree list`). It cannot reach a path the file RPCs would refuse to read, and it answers the
   same codes for an unknown token, an unknown project or an unlisted worktree.
2. **Refuse a `rel_path` that leaves the worktree**, with the same shape check the file RPCs apply
   (`validate_rel_path_shape`: no `..`, no absolute path); the refusal is `InvalidArgument`.
3. **Require the index daemon.** Without an `index_daemon:` section the daemon holds no
   `IndexDaemonRegistry`, so the handler has no channel source and every method answers `FailedPrecondition` naming that section. There is
   no fallback to `tddy_lsp_executor` or any other language server: two indexes answering the same
   pane would disagree.
4. **Dial it.** The channel source — in the daemon, `IndexDaemonRegistry::connect` — starts the index daemon on the first request and returns
   a channel to it. A start or dial failure is `Unavailable`. Nothing is started for a request that
   failed an earlier step.
5. **Forward** a `code_index.CodeIndexService` call with the listed worktree as `workspace_root`, the
   `rel_path` as `file`, and the position unchanged: both services speak one-based lines and one-based
   byte columns.
6. **Map the answer back.** Each index `CodeLocation{file, range, outside_root}` becomes a
   `CodeLocation{rel_path, range, outside_worktree}`. A refusal from the index (a non-`.rs` file, an
   unreadable file, a language-server failure) reaches the caller with the index's status.

`IndexDaemonRegistry::connect` has this service as its production caller. `tddy-index-daemon` is a
dependency of `tddy-daemon-rpc` (for the generated `code_index` client), and only a dev-dependency of
this crate.

## Warming a session's index

A started claude-cli, cursor-cli or workspace session announces its worktree to the host's
`SessionWorktreeObserver`; the daemon's observer then runs `code_index.Warm` for that worktree in the
background, through the same registry channel every navigation request uses (the first warm starts the
index daemon). Starting a session never waits on it. Progress is recorded per session id and read by
`WatchCodeIndex`; a warm that fails is recorded as the session's last progress with its reason. The
mechanics are in
[tddy-daemon-rpc's code index warm-up](../../tddy-daemon-rpc/docs/architecture.md#code-index-warm-up).

Only a worktree with a `Cargo.toml` at its root is warmed, because the index serves Rust. A session
whose start does not announce its worktree is never warmed — the sandboxed, tool and split starts, and
children spawned by a PR-stack orchestrator or a grill-me conversation — and its index loads on its
first navigation request.

## Who may watch

`WatchCodeIndex` is keyed by session id, not by a worktree path, and applies the worktree service's
`resolve_owned_session_dir`: token → OS user → `<sessions base>/sessions/<id>` must exist. A session
of another user and one that does not exist answer the same `NotFound`. It is the one ownership model
the worktree service's `RestoreSessionWorktree` shares.

## Transport

The service is registered among the daemon's RPC entries, so it is reachable over the transports the
web reaches the daemon on. It is generated with an RPC-server pass only (no tonic adapter): nothing reaches it over the
local gRPC socket.

## Testing

The handler's unit tests (path shape, location mapping) live in `tddy-daemon-rpc`;
`tests/code_navigation_acceptance.rs` here runs the production `IndexDaemonRegistry` against a stand-in
program that binds a fake `code_index` server over a Unix socket, so the lazy start, readiness wait
and dial are the production ones.

| Test | Pins |
|---|---|
| a definition request is forwarded for the session worktree | the fake index is asked once with `workspace_root` = the listed worktree and `file` = `rel_path`; the answer maps to `CodeLocation` |
| a worktree not listed for the project is refused | `FailedPrecondition`, the fake is never asked, the registry is never started |
| no `index_daemon:` section | `FailedPrecondition` naming `index_daemon` |
| the first request starts the index daemon | the stand-in is launched with `--grpc-uds <socket>` and the registry reports that socket running |

`tests/code_index_warmup_acceptance.rs` (the same stand-in, with a fake `Warm` stream the test drives):

| Test | Pins |
|---|---|
| a session on a Rust worktree starts warm once its worktree exists | `warm_for_session` starts a warm for the worktree; `WatchCodeIndex` joins it |
| `WatchCodeIndex` delivers phase, percentage and ready | the stream carries the fake's progress and ends after `ready` |
| without an index daemon no warm starts | no warm, no progress, and `WatchCodeIndex` ends empty |
| watching a session nothing warmed | the stream ends at once with nothing |
| a warm failure is reported with its reason | the last message carries `error`, and the warm does not panic |
| another user's watch is refused | `NotFound`, the same as for a missing session, and nothing is recorded for the id |

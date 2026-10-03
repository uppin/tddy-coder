# `code_navigation.CodeNavigationService`

Go-to-definition, references and hover for the web's [session code pane](../../../docs/ft/web/session-code-pane.md),
served by `tddy-daemon` and answered by the warm code-intelligence index. The service owns no index:
it authorises a request, then forwards it.

| RPC | Purpose |
|-----|---------|
| `Definition` | Where the symbol at a position of a worktree file is defined |
| `References` | Every reference to that symbol, its declaration included |
| `Hover` | The language server's hover markdown for it; unset when there is none |

The wire contract is [`tddy-service/proto/code_navigation.proto`](../../tddy-service/docs/code-navigation-proto.md).
`CodeNavigationServiceImpl` lives in `src/code_navigation.rs`, and `runtime.rs` registers its entry
beside the worktree service it borrows.

## The path every request takes

1. **Authorise exactly as `WorktreeService` does.** The service holds the daemon's
   `WorktreeServiceImpl` and calls its `resolve_listed_worktree` (session token → GitHub user → mapped
   OS user → the project's main repo on this host → the `worktree_path` must appear in that repo's
   `git worktree list`). It cannot reach a path the file RPCs would refuse to read, and it answers the
   same codes for an unknown token, an unknown project or an unlisted worktree.
2. **Refuse a `rel_path` that leaves the worktree**, with the same shape check the file RPCs apply
   (`validate_rel_path_shape`: no `..`, no absolute path); the refusal is `InvalidArgument`.
3. **Require the index daemon.** Without an `index_daemon:` section the daemon holds no
   `IndexDaemonRegistry`, and every method answers `FailedPrecondition` naming that section. There is
   no fallback to `tddy_lsp_executor` or any other language server: two indexes answering the same
   pane would disagree.
4. **Dial it.** `IndexDaemonRegistry::connect` starts the index daemon on the first request and returns
   a channel to it. A start or dial failure is `Unavailable`. Nothing is started for a request that
   failed an earlier step.
5. **Forward** a `code_index.CodeIndexService` call with the listed worktree as `workspace_root`, the
   `rel_path` as `file`, and the position unchanged: both services speak one-based lines and one-based
   byte columns.
6. **Map the answer back.** Each index `CodeLocation{file, range, outside_root}` becomes a
   `CodeLocation{rel_path, range, outside_worktree}`. A refusal from the index (a non-`.rs` file, an
   unreadable file, a language-server failure) reaches the caller with the index's status.

`tddy-index-daemon` is therefore a dependency of this crate, and `IndexDaemonRegistry::connect` has
this service as its production caller.

## Transport

The service is registered among the daemon's RPC entries, so it is reachable over the transports the
web reaches the daemon on. It is generated with an RPC-server pass only (no tonic adapter): nothing reaches it over the
local gRPC socket.

## Testing

`tests/code_navigation_acceptance.rs` runs the production `IndexDaemonRegistry` against a stand-in
program that binds a fake `code_index` server over a Unix socket, so the lazy start, readiness wait
and dial are the production ones.

| Test | Pins |
|---|---|
| a definition request is forwarded for the session worktree | the fake index is asked once with `workspace_root` = the listed worktree and `file` = `rel_path`; the answer maps to `CodeLocation` |
| a worktree not listed for the project is refused | `FailedPrecondition`, the fake is never asked, the registry is never started |
| no `index_daemon:` section | `FailedPrecondition` naming `index_daemon` |
| the first request starts the index daemon | the stand-in is launched with `--grpc-uds <socket>` and the registry reports that socket running |

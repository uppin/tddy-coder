# tddy-session-lifecycle

A session's whole life on a daemon: starting, connecting, resuming, signalling and deleting it,
served as `session.SessionService`, and the RPC host (`DaemonSessionHost`) that also serves the
session-files, session-agents, activity, terminal and demo-VM families. The Project, Catalog,
ExecTool and PR-stack families are served above this crate, by
[`tddy-daemon-rpc`](../tddy-daemon-rpc/README.md), and reached through the `DaemonRpcFamilies` port.

Extracted from `tddy-daemon` by the `#unbundle` stack; destructured in place by `#carve` 14/15
([#524](https://github.com/uppin/tddy-coder/pull/524)), so no file is over 500 production lines and
each duplicate has one definition. Its host-free topics live in the crates below it, behind facades
([#526](https://github.com/uppin/tddy-coder/pull/526)); the host-bound rest is converted to per-topic
ports and moved by `#carve` 16–21
([#531](https://github.com/uppin/tddy-coder/pull/531)–[#536](https://github.com/uppin/tddy-coder/pull/536)).

## Conversation worktrees

`LocalExecTools::run_exec_tool_locally` — the one route every exec tool takes — runs a call that
carries a `conversation_id` through `tddy_subagent_worktree::run_in_conversation` (no branch inside
`stream_execute_tool`), and merges the `worktreeChange` into its result.

`run_conversation_worktree_op` (`connection_service/conversation_worktree_op.rs`) serves every
`ConversationWorktree` operation — `Pull`, `PullRange`, `Remove`, `Reset`, `Diff`, `Sync` — with one
helper per op, for both the exec-tool RPC and the jail's host bridge. `Sync` merges the session
worktree's current files into the conversation's branch before a turn and answers
`{"sync": {commit, files, lines, paths, morePaths}}`, `{"sync": null}` (no worktree, nothing new, or a
recorded merge that changed no file), or `{"conflicts": [≤ SYNC_NOTICE_PATHS paths], "moreConflicts": n}`
with nothing moved; a merge and a conflict are logged at `info`.

**The host bridge is bound to its jail's session.** Each jail gets its own `DaemonRpcHandler`,
built by `sandbox_rpc_handler(session_id, session_dir)` at every jail launch
(`svc_start_sandboxed_cursor_cli_session.rs`, `jail_launch_steps.rs`, `relaunch_jail_steps.rs`) and
carrying a `BoundJailSession { session_id, session_dir }` — the session the daemon built the jail
for, with the session directory the daemon itself resolved under the session owner's sessions base.
The host keeps only a `Weak<DaemonSessionHost>` (`install_sandbox_rpc_bridge`).
`conversation_worktree_from_jail` refuses a request naming any other session with `PermissionDenied`
(and a warning in the log) — the runner rewrites every request to its own session, so one that names
another did not come through the runner — and reads the worktree from the bound `session_dir` through
`workspace_session::resolve_worktree_root_in_session_dir`, the same `.session.yaml` lookup the token
route's `resolve_worktree_root_for_session` makes. The runner's own rebinding
(`tddy-sandbox-runner`'s `conversation_root::bind_to_this_session`) is the second line of defence.
`Sync` reads the session's uncommitted files into a worktree the bridge's `Diff` can read, so this
binding is what keeps the bridge from being a read path into another session.
`agent_tool_reads_the_clone` delegates to `ToolEffect::of`: one read-only classifier.

## Quick Start

```bash
./test -p tddy-session-lifecycle
```

Twenty-two tests are red on a developer host for environmental reasons (the sandboxed-start suites
never get a sandbox RPC bridge, and the `session_sync` suite needs `tddy-remote-git-repo` built). The full command and the list are in
[docs/test-suites.md § What a local run shows](docs/test-suites.md#what-a-local-run-shows).

## Documentation

| Doc | What it covers |
|---|---|
| [docs/session-service.md](docs/session-service.md) | the eight `session.SessionService` RPCs, `TaskRegistry` ownership, the `DaemonRpcFamilies` port, the shared components the RPC handlers above this crate use, the presenter observer, the transports |
| [docs/module-layout.md](docs/module-layout.md) | how `src/` is organised: `connection_service`'s topic files and step modules, the sandboxed launch steps, the ports and peer-routed wrappers, the host builders, the `service_util` helpers, the facades over the modules below this crate, the definitions taken from lower crates, the topics and the coupling |
| [docs/test-suites.md](docs/test-suites.md) | the 56 integration suites, and where a new one goes |
| [docs/code-issues/](docs/code-issues/) | the open analyzer and structural findings, one file each |
| [docs/changesets/](docs/changesets/) | change history, one file per change |

Product docs: [claude-cli-session.md](../../docs/ft/daemon/claude-cli-session.md),
[cursor-cli-session.md](../../docs/ft/daemon/cursor-cli-session.md).

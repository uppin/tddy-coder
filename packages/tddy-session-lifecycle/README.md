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

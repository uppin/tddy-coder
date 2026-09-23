# tddy-daemon-rpc

The daemon's RPC-family handlers for `project.ProjectService`, `catalog.CatalogService`,
`exec_tools.ExecToolService` and `pr_stack.PrStackService` — one struct per family, each holding only
the session-host state that family reads.

## Quick Start

### Build
```bash
cargo build -p tddy-daemon-rpc
```

### Test
```bash
./test -p tddy-daemon-rpc
```

## Architecture

The crate sits **above** `tddy-session-lifecycle`, the way `tddy-daemon` does: each handler is built
from a `DaemonSessionHost` by `from_host`, shares the host's state through clones of its `Arc`s, and
never holds the host. `RpcHandlers` bundles the four for the composition root and is installed back
on the host as its `DaemonRpcFamilies` port, which is the only way session code reaches these
families. The edge is one-way: nothing below this crate depends on it, not even as a
dev-dependency.

## Documentation

### Technical implementation (how)
- [Architecture](./docs/architecture.md) — the handlers, `RpcHandlers`, the port, the one-way edge,
  tests
- [Code issues](./docs/code-issues/) — open analyzer findings in this crate
- [Changesets](./docs/changesets/) — applied changeset history

### Product requirements (what)
- [Project concept](../../docs/ft/daemon/project-concept.md)
- [PR stacking](../../docs/ft/coder/pr-stacking.md)
- [Specialized subagents](../../docs/ft/coder/specialized-subagents.md)

## Related Packages
- [tddy-session-lifecycle](../tddy-session-lifecycle/docs/session-service.md) — the host these
  handlers are built from, and the `DaemonRpcFamilies` port
- [tddy-pr-stack](../tddy-pr-stack/docs/architecture.md) — defines `PrStackHandler` and
  `PrStackServiceImpl`
- [tddy-projects](../tddy-projects/docs/project-service.md),
  [tddy-discovery](../tddy-discovery/docs/catalog-service.md),
  [tddy-tool-engine](../tddy-tool-engine/README.md) — the other three families' traits and service
  adapters
- [tddy-daemon](../tddy-daemon/docs/daemon-endpoint.md) — the composition root that installs
  `RpcHandlers`

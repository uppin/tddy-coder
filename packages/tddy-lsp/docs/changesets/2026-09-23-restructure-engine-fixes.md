# 2026-09-23 — The latest server status is kept for every reader

**Type:** Fix · `#carve` 13/15, PR [#527](https://github.com/uppin/tddy-coder/pull/527)
Cross-package entry: [`docs/dev/changesets/2026-09-23-restructure-engine-fixes.md`](../../../../docs/dev/changesets/2026-09-23-restructure-engine-fixes.md)

`LspClient::server_status` returns the last `experimental/serverStatus` the server sent, verbatim. The notification sink records it apart from the backlog, so neither a drainer nor a backlog overflowed by progress can take it. A status is sent only on a transition, so without this a second consumer of a warm server could never learn its health — which is what `tddy-code-restructuring`'s health gate needs on the index daemon. See [Reusable LSP](../../../../docs/ft/coder/reusable-lsp.md).

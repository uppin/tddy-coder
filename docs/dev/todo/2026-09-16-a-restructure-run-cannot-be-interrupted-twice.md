# 2026-09-16 — A restructure run wedged inside one request cannot be `^C`d again

**Category:** Future enhancement
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M2

`tddy-tools restructure` now installs a `SIGINT` handler so `^C` cancels the run — the replacement
for the withdrawn `--indexing-budget`, since a wait ends on readiness or on its caller stopping and
nothing else.

Taking `SIGINT` replaces its default disposition, so a run wedged *inside a single in-flight LSP
request* cannot be interrupted again until that request's liveness bound expires
(`ONE_REQUEST_LIVENESS`, 600s). The token is only looked at between requests: the retry loop checks
it, a request in flight does not.

`SIGTERM` is deliberately untouched, so `kill` still works immediately.

## Why the obvious fix was not taken

A second `^C` hard-exiting means `std::process::exit` in a library and new CLI behaviour, which the
milestone's brief ruled out. The deeper fix is `backends/lsp_bridge.rs`'s
`Handle::current().block_on` becoming cancellation-aware — it is what makes a single request opaque
to the token — and that would remove the need for `ONE_REQUEST_LIVENESS` entirely.

Either is a real improvement; the second is the one that makes the bound unnecessary rather than
merely escapable.

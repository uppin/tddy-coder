# 2026-09-16 — `Warm.ready` means "a live server holds this root", not "the graph is loaded"

**Category:** Defect
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M4a

`code_index.CodeIndexService`'s `Warm` streams `IndexProgress` and ends with `ready: true`. The
schema's comment says the root's crate graph is loaded and queryable. What the field actually means
is weaker: a live language server holds that root. `Warm` returns as soon as
`LspRegistry::get_or_spawn` yields a server, without waiting for the graph.

Measured: a `Warm` of a real three-crate workspace returned in **0.34s**, while the first `Anchors`
on the same root — which goes through `RustBackend::ensure_indexed` and therefore does wait — took
**2.1s**. So `ready` can be true while the next request still pays the whole load.

This was found by writing the production test for index reuse *against* `Warm`, which passed in 0.34s
having proven nothing. The test now probes `Anchors` instead, and says so in a comment.

## Why it was not fixed

Two private things stand in the way, and neither is a small change:

- `RustBackend::ensure_indexed` (`packages/tddy-code-restructuring/src/backends/rust.rs`) is the
  probe that decides readiness properly — private, and URI-scoped rather than root-scoped.
- The server's own `$/progress` is reachable only through `LspClient::drain_notifications`
  (`packages/tddy-lsp/src/client.rs`), which is **destructive and single-consumer**. Forwarding
  phases from there would steal them from a concurrent operation on the same root, which is folding
  the same queue into its own account.

## What closing it would take

`drain_notifications` becoming non-destructive — a `broadcast` or `watch` in place of a drain — so a
status reader and a running operation can both see progress. That is a design change in `tddy-lsp`
affecting every consumer of the client, which is why it was not folded into a feature.

Until then, treat `ready` as "addressable", and read the daemon's own log for the real answer: it
reports `which has no index yet` versus `which is already warm` per request.

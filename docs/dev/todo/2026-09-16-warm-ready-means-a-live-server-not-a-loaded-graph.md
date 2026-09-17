# 2026-09-16 — `RustBackend::ensure_indexed` is the readiness probe, and it is private and URI-scoped

**Category:** Future enhancement
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M4a

## What has been closed

`Warm.ready` now means the root's crate graph is loaded, which is what the schema always claimed.
`tddy-lsp` publishes `LspClient::subscribe_notifications`, and `tddy-index-daemon` uses it: a
per-root latch (`graph.rs`) is fed by a watcher attached the moment a server is reached, folding
`$/progress` and `experimental/serverStatus` through `tddy-code-restructuring`'s own
`ServerChatter` — which is now published for the purpose rather than restated. `warm.rs` forwards
the server's phases into `IndexProgress` (`line`, `phase`, `percentage`, `furthest`) and ends
`ready: true` only once the latch says the graph is queryable.

A latch, rather than a per-request read of the notifications, because rust-analyzer reports
quiescence **on the transition and only on the transition**: a second `Warm` that waited to be told
would wait for ever. Pinned by `code_index_service_acceptance.rs` against `fake_lsp`'s new
`--loads-crate-graph` mode, and by `warm_index_production.rs` against a real rust-analyzer —
`a_warm_of_a_loaded_root_is_immediate_where_the_first_one_paid_for_the_graph`, which fails if
`ready` stops waiting for the graph.

## What remains

`RustBackend::ensure_indexed` is still the *strongest* readiness probe there is — it hovers until
the server answers, so it stands on an answer rather than on an extension — and it is still private
and URI-scoped rather than root-scoped. Two consequences:

- `Warm` stands on `experimental/serverStatus`, which is an extension. A server that never sent it
  would leave a warm waiting until its caller hung up. Every rust-analyzer this repo runs against
  sends it, and `ServerChatter::quiescent` is documented as "has not said so" rather than "is not
  loaded", but a probe would not need the extension at all.
- A watcher that falls behind is told how many notifications it lost and cannot know whether the
  transition was among them. It keeps waiting rather than guessing (`graph.rs`), and logs the loss.
  A probe would answer the question directly instead.

Closing this means a root-scoped readiness probe on `RustBackend`, public, which `Warm` would use as
the authority — keeping the notification stream as the narration it is good at.

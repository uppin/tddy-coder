# 2026-09-09 — The handshake, server errors and notifications a refactoring consumer needs

**Type:** Fix

A consumer that asks for refactorings needs more from `LspClient` than one that asks for
definitions, and three of those things were not reachable.

**The handshake is the caller's to choose.** `initialize` advertised an empty capability set,
hardcoded. `LaunchSpec::with_capabilities` and `with_initialization_options` now carry it, and
`LspClient::handshake()` returns what the server negotiated. This is load-bearing rather than
cosmetic: rust-analyzer returns **no code actions at all** to a client that advertised no
`codeAction` support — indistinguishable from a range that supports no refactoring — and it counts
positions in **utf-16** code units unless asked for utf-8, which silently misplaces every column on
any line outside ASCII.

**A JSON-RPC `error` reaches the caller** as `LspError::Server { code, message }`. The dispatcher read
`result` and defaulted to null, so an error arrived as a *successful empty answer*. Consumers
dispatch on the code: rust-analyzer's `ContentModified` (-32801) means "ask again", not "this
failed", and a caller that cannot tell them apart either retries forever or gives up on a live server.

**Server notifications are retained for a consumer to drain.** `drain_notifications` returns the ones
the client does not consume itself, oldest first, capped at 256 so an undrained backlog cannot grow
without bound. `$/progress` and `experimental/serverStatus` are the two that matter — during a load
that answers no requests they are the only account of what the server is doing.

`set_request_timeout` adjusts the per-request wait through `&self`, since consumers hold the client
behind an `Arc` from the registry. Ten seconds suits interactive queries; a code-action request
against a cold index does not fit in it.

Feature docs: [reusable-lsp.md](../../../docs/ft/coder/reusable-lsp.md),
[rust-code-restructuring.md](../../../docs/ft/coder/rust-code-restructuring.md).

5 tests added, including one asserting the launch spec's capabilities reach the server — the
assertion whose absence allowed the empty handshake. `tddy-lsp` 30/30.

# 2026-09-16 — `did_open` on an already-open document restarts its version sequence

**Category:** Defect
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M1

`LspClient` now owns per-URI document version state, so a caller that edits a document twice sends an
ascending sequence and a second caller does not begin again at 1. `did_open` is the exception: it
records the URI at version 1 whether or not it was already open.

That is deliberate and load-bearing. `LspRegistry::bind_target`
(`packages/tddy-lsp/src/registry.rs`) re-opens **every** `src` on every call, and
`tddy-lsp-executor` calls it per tool invocation (`packages/tddy-lsp-executor/src/lib.rs`), so
refusing a re-open would break that consumer's bind path. It is also not a regression — the previous
code always sent `"version": 1`.

But it is the same defect class the rest of M1 set out to fix: on a host that outlives one request, a
re-`did_open` without a `did_close` restarts a sequence the server has already advanced, and the
server is entitled to ignore edits that go backwards.

## What closing it would take

`bind_target` issuing a `did_change` for a URI that is already open, instead of re-opening it. That
is a behaviour change to `tddy-lsp-executor`'s path — the MCP `Lsp*` tools' staleness handling
currently *depends* on the re-open — so it needs that consumer's tests to move with it, and a decision
about what a tool call should do when the file on disk changed under an open document.

No test pins the current behaviour either way, deliberately: pinning it would make the wrong thing
harder to change.

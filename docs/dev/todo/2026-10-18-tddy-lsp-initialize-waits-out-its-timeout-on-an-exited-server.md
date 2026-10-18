# 2026-10-18 — `tddy-lsp`'s `initialize` waits out its 10 s timeout on a server that already exited

**Category:** Future enhancement
**Source:** `code-index-warm-on-demand` changeset — Technical Debt; observed in
`packages/tddy-index-daemon/tests/root_progress_acceptance.rs`
(`watching_a_root_whose_server_cannot_start_reports_it_failed_with_the_reason`)

## What is left

When a language server exits during startup, `tddy-lsp`'s client does not notice the closed stdout:
the `initialize` request waits for `DEFAULT_REQUEST_TIMEOUT` (10 s, `packages/tddy-lsp/src/client.rs`)
before it fails with "lsp server exited". So a root whose rust-analyzer cannot start reads *starting* /
*loading* for 10 s before the Code pane's strip and the session header show *Indexing failed — …*, and
the index-daemon test that pins the `FAILED` state takes ~10 s.

## Why it was deferred

Pre-existing `tddy-lsp` behaviour, not changed by the code-index work: that change only reports the
failure (`IndexState::FAILED` with `error`) once the client gives it. Failing pending requests when the
reader hits EOF is a `tddy-lsp` client change with its own tests, and every `tddy-lsp` consumer would
feel it.

## What would close it

In `tddy-lsp`'s reader task, on EOF (or the child's exit), fail every pending request at once with
`ServerExited` instead of leaving them to their timeouts; a `fake_lsp` mode that exits after reading
`initialize` pins it. The `root_progress_acceptance` failure test should then take milliseconds.

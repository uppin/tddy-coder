# 2026-10-23 — Extension verify refuses a language server that only pushes diagnostics

**Category:** Recorded debt from `reshape-extension-code-actions` (`#reshape` 23/24, PR #682)
**Source:** [2026-10-23-reshape-extension-code-actions.md](../changesets/2026-10-23-reshape-extension-code-actions.md) § Decisions
**Owner:** the `#reshape` sweep node

## What it does today

`ExtensionVerify` reads diagnostics by **pull** (`textDocument/diagnostic`, `LspClient::pull_diagnostics`)
after the before and after text is announced. A server that advertises no `diagnosticProvider` makes the
verify fail as `ServerDefect`. It is not verified by the parse alone. This was a deliberate choice: the
publish cache says nothing about the text last sent, and a verify that silently skipped diagnostics would
report a hold it never checked.

## Why it matters

Many servers still only publish (`textDocument/publishDiagnostics`). For those languages `restructure verify`
over the warm daemon cannot hold at all.

## Wanted

A push path that is still tied to the text: send the before text, wait for the publish whose `version`
matches it, then do the same for the after text. Fall back to nothing. A server that sends no versions stays
refused, and the refusal says why.

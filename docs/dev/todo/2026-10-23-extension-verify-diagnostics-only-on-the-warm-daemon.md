# 2026-10-23 — Extension verify reads new error diagnostics only on the warm index daemon

**Category:** Follow-up from `reshape-extension-code-actions` (`#reshape` 23/24, PR #682)
**Source:** [2026-10-23-reshape-extension-code-actions.md](../changesets/2026-10-23-reshape-extension-code-actions.md) § Decisions
**Owner:** the `#reshape` sweep node

## What is missing

R5 asks two things of an extension-language file a run touched: no new error diagnostic from its
language server, and no `ERROR` / `MISSING` node in its grammar's parse. Only one of the two front ends
asks both.

- **The cold CLI** (`tddy-tools restructure verify` with no `TDDY_INDEX_SOCKET`): `tddy-tools-cli`'s
  `installed_languages` builds `ExtensionVerify::for_languages(&languages, &[])`. It starts no language
  server for a verify, so the cold run judges by the parse alone.
- **The library entry** `tddy_index_daemon::build_code_index_entry_with_supports` builds its service with no
  `with_verifiable`, so a host that embeds the service through that entry verifies Rust only. No production
  host does that today: `tddy-daemon` forwards to the spawned `tddy-index-daemon` binary, and that binary's
  `wired` calls `with_verifiable`. Only tests reach the entry.

The warm daemon binary verifies over each touched language's warm server, so the full R5 check holds there.

## Why it was not done in #682

Starting a server per cold verify costs a cold start for every language touched, which is a design choice
the changeset did not make. The library entry has no production caller to wire.

## Wanted

- Either the cold `verify` starts the touched languages' servers (one `ServerBackend`-style launch per
  language, stopped at the end of the run), or it says on its output that diagnostics were not read.
- `build_code_index_entry_with_supports` takes the installed languages, as `wired` does, or is replaced by
  an entry built from `host_languages_launching`.

# Changeset: an extension language's server starts from tddy settings, and gives it rename and references

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (declarative language-server launch for extension languages)
**Stack**: `#reshape` 22/24, branch `feature/reshape/extension-language-server`, base `feature/reshape/backend-fn-sizes`, green wave 3.

## Initial Discovery

[2026-10-19-reshape-extension-language-server-initial-discovery.md](2026-10-19-reshape-extension-language-server-initial-discovery.md)

## Affected Packages

- `packages/tddy-zed-extensions`: `settings.rs`, `ExtensionSupport::launch`.
- `packages/tddy-lsp`: `workspace/configuration` answered from the spec's settings (`client/dispatch.rs:106`).
- `packages/tddy-code-restructuring`: `ServerBackend` (generic rename), composed with K20's `OutlineBackend`.
- `packages/tddy-index-daemon`: code-index queries route by support (`navigation.rs`), index state for
  extension-only worktrees.
- Docs that say non-Rust worktrees get no index: `packages/tddy-index-daemon/docs/code-index-service.md`,
  `packages/tddy-web/docs/session-start-and-indexing-progress.md`.

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-language-server.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-language-server.md)

## Summary

`<extensions dir>/settings.json` in Zed's `lsp` schema gives an extension language's server a command; with it the
language has `rename_symbol` and references, on the CLI and through the index daemon.

## Responsibility

| Symbol | Where |
|---|---|
| `ExtensionSettings` (`lsp.<id>.binary.{path,arguments,env}`, `initialization_options`, `settings`) | `tddy-zed-extensions/src/settings.rs` |
| `ExtensionSupport::launch()` → `LaunchSpec` (+ `workspace_configuration`) | `tddy-zed-extensions/src/support.rs` |
| `LaunchSpec::workspace_configuration: Value` | `tddy-lsp/src/allowlist.rs` |
| `ServerBackend` (`rename_symbol` via `textDocument/rename`) | `tddy-code-restructuring/src/backends/server.rs` |
| code-index routing by support | `tddy-index-daemon/src/navigation.rs`, `queries.rs` |

### The rules (the contract)

- **R1 — which server.** The manifest's `[language_servers.<id>]` whose `language(s)` names the language; several →
  the first not `opt_in_languages` for it.
- **R2 — command.** Only from `lsp.<id>.binary.path` (+ `arguments`, `env`). No PATH lookup (PRD F1 a).
- **R3 — missing command.** Any server-needing op is refused before a process starts:
  `language server <id> (extension <ext>) has no command: set lsp.<id>.binary.path in <file>`.
- **R4 — configuration.** `initialization_options` sent at `initialize`; `workspace/configuration` items answered from
  `settings` by their `section` path (null where absent); Rust's spec carries none (unchanged).
- **R5 — root.** The restructure / worktree root (PRD F2 a).
- **R6 — rename.** `rename_symbol` on an item anchor sends `textDocument/rename` at the entry's `name_at`; the returned
  `WorkspaceEdit` is the resolution.
- **R7 — daemon.** definition / references / hover for an extension-language file go to its server; an extension-only
  worktree whose language has a server reports an index state other than `NONE`.

## Boundaries

- No WASM (K24); code actions and verify are K23.

## Dependencies

- `feature/reshape/extension-outline` (K20) — **real edge**: `ExtensionSupport`, `OutlineBackend`, entry `name_at`.
- `feature/reshape/backend-fn-sizes` (K21) — line position.

## Draft PR contract

- Surface: the symbols above, bodies `todo!()` (`TODO(reshape-extension-language-server)`); `LaunchSpec` field real.
- Failing tests (fake LSP from `tddy-lsp`'s test support, configured through `settings.json`):
  `tddy-zed-extensions/tests/language_server_acceptance.rs`: `a_configured_server_renames_an_extension_symbol`,
  `the_server_receives_the_initialization_options`, `workspace_configuration_is_answered_from_settings`,
  `an_unconfigured_server_is_refused_before_anything_starts`, `an_opt_in_language_does_not_pick_the_server`;
  `tddy-index-daemon/tests/extension_references.rs`: `the_daemon_answers_references_for_an_extension_file`,
  `an_extension_only_worktree_has_an_index_state`.

## Green wave

**Wave:** 3 of 4.
**Greenable independently:** no — needs `extension-outline` (K20) green.
**Concurrent with:** none.
**Blocks:** `extension-code-actions` (K23), `extension-wasm-command` (K24).

## Successor PRs

`feature/reshape/extension-code-actions` (K23).

## Scope

- [ ] USER REVIEW (ticked at `/green` start)
- [ ] Settings + launch spec
- [ ] `workspace/configuration` from settings
- [ ] Generic rename
- [ ] Daemon routing + index state
- [ ] Docs: `## Language servers` in zed-extensions.md; index-daemon and tddy-web docs corrected
- [ ] Code Quality

## Testing plan

Library and daemon level against the fake LSP; no real extension server.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._

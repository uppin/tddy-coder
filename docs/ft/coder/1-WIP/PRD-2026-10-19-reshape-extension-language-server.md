# An extension language's server starts from tddy settings, and gives it rename and references - PRD

**Date**: 2026-10-19
**PRD Type**: New feature (declarative language-server launch for extension languages)
**Stack**: `#reshape` 22/24 (`feature/reshape/extension-language-server`, base `feature/reshape/backend-fn-sizes`; real
parent `feature/reshape/extension-outline` — wave 3; `extension-code-actions` and `extension-wasm-command` wait on it)

## Affected Features

- `docs/ft/coder/zed-extensions.md`: new section `## Language servers` and the settings file.
- [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md): extension languages are served too.

## Summary

- An extension's `[language_servers.<id>]` entries say **which** server serves which of its languages; Zed gets the
  **command** from the extension's WASM. Without WASM (the default), tddy takes it from its settings, in the shape of
  Zed's own `lsp` setting so a block can be copied from Zed's `settings.json`:
  ```json
  { "lsp": { "elixir-ls": { "binary": { "path": "/opt/elixir-ls/language_server.sh", "arguments": [], "env": {} },
                             "initialization_options": {}, "settings": {} } } }
  ```
  in `<extensions dir>/settings.json`.
- With a binary set, the language's support gets a `LaunchSpec` (K11): program, arguments, env, the
  initialization options, and `settings` answered to `workspace/configuration`.
- Without one, any operation that needs the server is refused before anything starts:
  `language server elixir-ls (extension elixir) has no command: set lsp.elixir-ls.binary.path in <file>`.
- With a server, an extension language has **LSP rename** (`rename_symbol`) and **references** — through the
  restructure engine and through the index daemon's code-index queries (definition, references, hover), which today
  refuse non-`.rs` files.

## Background

- `LaunchSpec` already carries `program, args, env, capabilities, initialization_options` (`allowlist.rs:26`);
  `workspace/configuration` is answered with nulls today (`client/dispatch.rs:106`).
- Zed's own precedence: a user-set `lsp.<id>.binary.path` is used before the extension's
  `language_server_command`. Declarative tddy is that first branch alone.
- The workspace root for an extension language is the restructure root (no manifest rule is known) — F2.

## Proposed Changes

### What's Changing

- **`tddy-zed-extensions`**: `ExtensionSettings` (reads `settings.json`; unknown keys ignored), `server_for(language)`
  from the manifest's `language_servers`, `ExtensionSupport::launch()` returning the `LaunchSpec`.
- **`tddy-lsp`**: `workspace/configuration` is answered from the spec's `settings` (sections by `section` path) instead
  of nulls; Rust keeps sending none (unchanged behaviour).
- **`ServerBackend`** (generic, in `tddy-code-restructuring`): `rename_symbol` via `textDocument/rename` on an item
  anchor's name position; returns the `WorkspaceEdit` as the `Resolution`. Composed with K20's `OutlineBackend`.
- **Index daemon**: the code-index LSP queries route by the file's support instead of refusing non-`.rs`
  (`navigation.rs` constants become the support's); a worktree whose only supports are extension languages gets an
  index state other than `NONE` when one of them has a server.
- **No PATH guess** (F1): a server name is not looked up on `PATH`.

### What's Staying the Same

- Rust launch, capabilities and settings. Extension WASM is not run (K24).

## Impact Analysis

- Packages: `tddy-zed-extensions`, `tddy-lsp` (`workspace/configuration`), `tddy-code-restructuring` (generic
  backend), `tddy-index-daemon` (routing). Docs that say non-Rust worktrees get no index change.

## Acceptance Criteria

- [ ] With a fixture extension declaring server `fake-ls` and `settings.json` pointing `binary.path` at the repo's fake
      LSP, `rename_symbol` on an extension-language file returns the server's edit and the run applies it.
- [ ] The server receives the configured `initialization_options`, and its `workspace/configuration` request is
      answered from `settings`.
- [ ] Without a binary setting the operation is refused with the message above, before any process starts.
- [ ] The index daemon answers references for an extension-language file.

## Open questions

- **F1 — no command configured.** (a) Refuse and name the setting (recommended; no fallback without consent);
  (b) look the server id up on `PATH` (you approved this in the decomposition, but a server id is often not its binary
  name: `pyright` vs `pyright-langserver`).
- **F2 — workspace root for an extension language.** (a) The restructure / worktree root (recommended); (b) the
  nearest directory holding a file the language's `config.toml` lists (Zed has no such field; tddy would invent one).
- **F3 — settings location.** (a) `<extensions dir>/settings.json`, Zed's `lsp` schema (recommended); (b) the daemon's
  `daemon.yaml`.

## References

- Zed: `[language_servers.<id>]` manifest entries; `lsp.<id>.binary` settings.
- Discovery §3 (LaunchSpec, `workspace/configuration`), §4 (index daemon refuses non-`.rs`).

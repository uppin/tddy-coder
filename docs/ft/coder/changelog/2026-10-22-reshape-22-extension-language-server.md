# 2026-10-22 — an extension language's server starts from tddy settings, with rename, references and an index

`#reshape` 22/24 ([#681](https://github.com/uppin/tddy-coder/pull/681)).

- **Configured in Zed's shape.** `<extensions dir>/settings.json` takes Zed's `lsp.<id>` block — `binary.path`,
  `arguments`, `env`, `initialization_options`, `settings` — for a server an installed extension declares, and
  `languages.<Name>.language_servers` opts a language into an opt-in server.
- **No command, no server.** A declared server without `binary.path` is never guessed from `PATH`: every operation
  needing it is refused, naming the setting, before anything starts.
- **Rename.** `rename_symbol` on an extension-language item renames it through the language's server, from the
  cold CLI and through the index daemon.
- **References, definition, hover** for an extension-language file through the index daemon.
- **An index.** A worktree with no `Cargo.toml` whose language has a configured server has a code index, in the
  session header and Code pane as in the daemon.

Feature docs: [Zed extensions § Language servers](../zed-extensions.md#language-servers),
[Warm code-intelligence daemon](../warm-code-intelligence-daemon.md).

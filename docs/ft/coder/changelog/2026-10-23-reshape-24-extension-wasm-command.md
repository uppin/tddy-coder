# 2026-10-23 — an opted-in extension's WASM names its language server's command

`#reshape` 24/24 ([#683](https://github.com/uppin/tddy-coder/pull/683)).

- **Opt-in only.** `tddy-tools extensions enable-wasm <id>` lets an installed extension's `extension.wasm` run;
  `disable-wasm` stops it. By default no extension code runs, and a new version of an opted-in extension stays
  opted in.
- **The WASM names the server.** For a declared server with no `lsp.<id>.binary.path`, an opted-in extension's
  `language_server_command`, initialization options and workspace configuration start it. A configured
  `binary.path` still wins.
- **A minimal host.** The worktree, settings and platform are answered; every download, install, network or
  process call is refused with `tddy does not download language servers; install <server> and set
  lsp.<server>.binary.path`, and the server is refused with those words.
- **Bounded.** `zed_extension_api` 0.6.x only, refused with the range otherwise; each call is stopped after
  10 seconds.

Feature docs: [Zed extensions § Extensions with code (opt-in)](../zed-extensions.md#extensions-with-code-opt-in).

# 2026-10-22 — an installed Zed extension's languages get anchors and outlines from its grammar

`#reshape` 20/24 ([#679](https://github.com/uppin/tddy-coder/pull/679)).

- **Extension languages are restructure languages.** Every language an installed Zed extension declares claims the
  files its `path_suffixes` name (a built-in keeps a suffix it shares), loads its `grammars/<grammar>.wasm` through
  tree-sitter's WASM loader, and reads its outline from the extension's `outline.scm`.
- **Anchors, as for Rust.** `restructure anchors --items` / `--at`, a plan's `item` / `items` anchors and a static
  `check`'s fingerprint test work on such a file, with item paths `<file>::<context…>::<name>` and Rust's refusals.
  No rust-analyzer is started for it, from the cold CLI or the index daemon.
- **No server yet.** Any operation on such a file is refused with `UnsupportedOp` naming the language.
- **A broken grammar is reported once** — `extension <id>: grammar <g> did not load: <reason>` — and every other
  extension still loads.

Feature docs: [Zed extensions § Languages from an extension](../zed-extensions.md#languages-from-an-extension),
[Rust code restructuring § Languages](../rust-code-restructuring.md#languages).

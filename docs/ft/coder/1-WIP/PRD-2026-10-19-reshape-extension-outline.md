# An installed Zed extension's languages get anchors and outlines from its grammar - PRD

**Date**: 2026-10-19
**PRD Type**: New feature (the declarative Zed adapter, outline tier)
**Stack**: `#reshape` 20/24 (`feature/reshape/extension-outline`, base `feature/reshape/extension-install`; real parent
`feature/reshape/language-support` — wave 2, leads it: `extension-language-server`, `extension-code-actions`,
`extension-wasm-command` wait on it)

## Affected Features

- **`docs/ft/coder/zed-extensions.md`** (from K19): new section `## Languages from an extension`.
- [Rust code restructuring](../rust-code-restructuring.md) `## Languages`: extension languages are supports too;
  which operations they have.

## Summary

Every language an installed extension declares becomes a **language support** (K11):

- it claims files by its `config.toml` `path_suffixes` (a built-in still wins a shared suffix — K11 F2);
- its grammar is loaded from the shipped `grammars/<grammar>.wasm` through tree-sitter's WASM loader (developer
  decision 2026-10-19), with no language server;
- its **outline** comes from the extension's `outline.scm`: each `@item` is an entry, `@name` its name, `@context` the
  enclosing entries;
- so for its files, `restructure anchors` emits item anchors and `anchor_for` resolves `items` / `item` anchors, and
  static `check` verifies an anchor's fingerprint — exactly as for Rust, against the outline instead of
  rust-analyzer's `documentSymbol`.

Operations that need a server (rename, code actions, references) are refused for such a language with
`UnsupportedOp { backend: "<language>", op }` until `extension-language-server` (K22) gives it one.

## Background

- Rust anchors come from `documentSymbol` (`settled_outline`, rust.rs:1736) plus Cargo module paths
  (`item_anchor.rs:46`). An extension language has neither; Zed itself builds its outline panel from `outline.scm`.
- The anchor traits are already language-agnostic: `ItemResolver`, `ItemAtResolver`, `anchor_for`
  (`registry.rs:75–110`).

## Proposed Changes

### What's Changing

- **`tddy-zed-extensions`**: `ExtensionLanguage` (name, grammar, path suffixes, queries) loaded from each
  `installed/<id>/languages/<lang>/`; `ExtensionSupport: LanguageSupport` (id = the language name lower-cased and
  kebab-cased, e.g. `"elixir"`); `OutlineBackend: LanguageBackend` with `item_resolver`, `item_locator`, `anchor_for`.
- **Grammar loading**: `tree_sitter::WasmStore` + `Language` from the `.wasm` bytes, one engine per process, a grammar
  loaded once per run. Dependencies `tree-sitter` (feature `wasm`) and `wasmtime` (consented).
- **Outline**: run `outline.scm` over the parse tree; an entry's span is its `@item` node, extended over leading
  `@annotation` nodes; its path is the `@context` names outermost first, then `@name`.
- **Item paths for extension languages**: `<file>::<context…>::<name>` written relative to the workspace root (F1).
- **Registration**: `SupportRegistry::with_installed_extensions(dir)` after the built-ins, on the cold CLI path
  (`restructure_cli.rs`) and in the index daemon (`main.rs`), so `restructure anchors` works either way.
- **A load failure is not fatal to other languages**: an extension whose grammar fails to load is reported once per
  run (`extension <id>: grammar <g> did not load: <reason>`) and its languages claim nothing.

### What's Staying the Same

- Rust anchors, outlines and errors.
- No language server is started for an extension language.

## Impact Analysis

- Packages: `tddy-zed-extensions`, `tddy-code-restructuring` (registration only), `tddy-index-daemon` (registration).
- wasmtime is large: compile time and binary size of `tddy-tools` and `tddy-index-daemon` grow (measure in the node).

## Acceptance Criteria

- [ ] With a fixture extension installed, `restructure anchors --items` on a file of its language prints one anchor
      per outline entry with its item path and fingerprint.
- [ ] A plan line anchored on two adjacent entries resolves through `anchor_for`; non-adjacent entries are refused as
      for Rust.
- [ ] Static `check` reports a stale fingerprint on an extension-language anchor.
- [ ] `rename_symbol` on such a file is refused with `UnsupportedOp` naming the language.
- [ ] A broken grammar WASM leaves other languages working and is reported once.
- [ ] The same anchors come back through the index daemon as through the cold CLI.

## Open questions

- **F1 — item path for an extension language.** (a) `<relative file>::<context>::<name>` (recommended: no module
  system is known); (b) `<context>::<name>` with the file only in the anchor.
- **F2 — test fixture grammar.** The suite needs one real grammar WASM in the repo. (a) Vendor one small,
  MIT-licensed grammar built with the wasi-sdk (e.g. tree-sitter-ini or tree-sitter-json, ~50–200 KB) under
  `tests/fixtures/`, with its licence (recommended); (b) download in a build script (network in builds).
- **F3 — which queries this node reads.** (a) `outline.scm` only (recommended); (b) also `textobjects.scm` for
  function bodies.

## References

- Zed docs `extensions/languages` (outline captures `@item @name @context @context.extra @annotation`).
- Discovery §5 (anchors), Zed facts (installed layout, grammars).

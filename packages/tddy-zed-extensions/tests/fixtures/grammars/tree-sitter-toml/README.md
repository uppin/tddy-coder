# tree-sitter-toml grammar (test fixture)

`toml.wasm` is the TOML grammar as Zed ships it, vendored so the `extension-outline` suites load a
real tree-sitter grammar through tree-sitter's WASM loader without a network or a wasi-sdk build.

| | |
|---|---|
| Grammar | [tree-sitter/tree-sitter-toml](https://github.com/tree-sitter/tree-sitter-toml) at `342d9be207c2dba869b9967124c679b5e6fd0ebe` (grammar ABI 13) |
| Licence | MIT, Copyright (c) Ika — [`LICENSE`](LICENSE), copied from the grammar repository at that commit |
| Taken from | `grammars/toml.wasm` of the Zed extension `toml` 1.0.3, downloaded 2026-10-20 from `https://api.zed.dev/extensions/toml/download?min_schema_version=0&max_schema_version=1&min_wasm_api_version=0.6.0&max_wasm_api_version=0.7.0` (archive sha256 `ceaa444974e4d6da8b9c77ba8fea3ea8ef0953eaf67a5688b5b1d9245d35e68e`) |
| Size | 26,937 bytes |
| sha256 | `de6823f0a8722d857e097a98b514ca98e04a45db1138991ab29f0eeaec72b86a` |
| Loads with | `tree-sitter` 0.26.13, feature `wasm` (wasmtime 36) |

Only the compiled grammar is vendored. The extension's own query files (Apache-2.0, zed-extensions/toml)
are not: the fixture extension `../../demo-toml/` carries its own `outline.scm`.

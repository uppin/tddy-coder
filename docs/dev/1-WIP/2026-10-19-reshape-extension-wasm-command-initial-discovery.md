# #reshape 24/24 (extension-wasm-command) — initial discovery

## Exploration 1 — whole-work discovery (2026-10-19)


**Temporary.** Copied into each new node's `*-initial-discovery.md` as Exploration 1; deleted once
every node has its companion (ask the developer first). Not a changeset companion.

### Combined conclusions

1. **The plug-in seam exists but is narrow.** `LanguageBackend` + `BackendRegistry`
   (`tddy-code-restructuring/src/registry.rs:37,113`) match a file to a backend by extension only;
   registries are built in exactly two functions (`runner/entry_points.rs:93,125`), with callers in
   the runner and `tddy-index-daemon` (`apply.rs:123`, `queries.rs:96`, `plan_upkeep.rs:53`).
2. **Two `Language` enums, both `{ Rust }`:** `registry::Language` (errors only, not re-exported)
   and `tddy_lsp::Language` (`allowlist.rs:11`, keys `LspKey`, `id() = "rust"`). `language-support`
   must collapse them into one open language id owned by `tddy-lsp`.
3. **Server launch is Rust-only and duplicated.** `LaunchSpec { program, args, env, capabilities,
   initialization_options }` (`allowlist.rs:26`) is the right shape; the production allow-list with
   real capabilities is built twice (`restructure_cli.rs:257`, `tddy-index-daemon/src/main.rs:208`)
   from `backends/rust.rs:199,265`; `rust_only()` is used by tddy-daemon, tddy-sandbox-app,
   tddy-lsp-executor. `languageId: "rust"` is hard-coded at `backends/rust/documents.rs:40` and
   `tddy-lsp/src/registry.rs:356`. The workspace root rule is Cargo-specific (`registry.rs:311`).
4. **The index daemon is Rust-only**: `navigation.rs:26,30` (`LANGUAGE_ID`, `SOURCE_EXTENSION`),
   `lib.rs:61` (`Cargo.toml`), `plan_upkeep.rs:77`; docs say non-`.rs` files are refused. Extension
   languages must work on **both** paths: the cold CLI and the warm daemon (`TDDY_INDEX_SOCKET`).
5. **Anchors are LSP-based for Rust** (`documentSymbol` via `settled_outline`, rust.rs:1736; RA
   symbol kinds 2/19) plus Cargo module paths (`item_anchor.rs:46`, `package_lookup.rs:8`). Extension
   anchors from tree-sitter + `outline.scm` are a **new** provider behind `ItemResolver` /
   `ItemAtResolver` / `anchor_for`.
6. **Verify is Rust-only** (`verify.rs`, statement multiset, `*.rs` filter at
   `runner/comparison.rs:79`) and bypasses `LanguageBackend`. A generic verify for extension
   languages is a new definition (open question for node 23).
7. **LSP coverage.** Typed: diagnostic, definition, references, hover, documentSymbol,
   workspace/symbol, document sync. Raw: codeAction, codeAction/resolve, rename, semanticTokens.
   **Not handled: `workspace/executeCommand`, server-initiated `workspace/applyEdit`** (answered
   `null`), which a generic code-action passthrough needs.
8. **`RefactorKind`** (`plan/refactor_kind.rs:14`, 32 variants) has no generic code-action kind;
   Rust dispatch is an if-chain in `resolve_opening` (rust.rs:1274–1440); `assisted_edit` already
   runs a code action by exact title.
9. **CLI:** `tddy-tools` `enum Subcommand` (`packages/tddy-tools/src/main.rs:32–99`, arms 138–163);
   args live in the owning crate (pattern: `restructure_args.rs`, `analyze_cli.rs`). An
   `Extensions(..)` variant fits there.
10. **Home dir:** env `TDDY_DATA_DIR`; `default_tddy_data_dir()` (`tddy-session-store/src/output/writer.rs:105`)
    is `tmp/.tddy` in debug, `None` in release, each caller falling back to `$HOME/.tddy`
    (`tddy-daemon/src/runtime.rs:583`). Extensions dir = `<data dir>/extensions`.
11. **Deps:** reqwest (tddy-tools and 14 others), toml (tddy-daemon-kernel only) present; flate2 in
    Cargo.lock but used by no package; **tar, tree-sitter, wasmtime absent** (consented 2026-10-19).
12. **Sizes:** registry.rs 390, backends/rust.rs 5,584 (+~40 submodules), tddy-lsp registry.rs 361,
    allowlist.rs 170.
13. **Backlog in the path:** `2026-10-17-restructuring-acceptance-suite-pays-a-rust-analyzer-start-per-test.md`
    (fake backend — declined until a consumer; extension fixtures become one),
    `2026-09-16-backends-rust-rs-is-4500-production-lines.md` and code-issue
    `oversized-file-backends-rust.md` (rust-backend-split dropped; still open),
    `2026-10-03-restructure-leftovers-…` (`backend_impl` carve), docs that non-Rust worktrees get no
    index (`tddy-index-daemon/docs/code-index-service.md:199`, `tddy-web/docs/session-start-and-indexing-progress.md:116`).

### Zed extension facts (Zed sources, 2026-10-19)

- Manifest (`crates/extension/src/extension_manifest.rs`): `ExtensionManifest { id, name, version,
  schema_version, description?, repository?, authors, lib: { kind?, version? }, themes, languages,
  grammars: {name → { repository, rev, path? }}, language_servers: {id → { name?, language(s),
  … }}, capabilities, … }`. Extensions without code are valid (themes, snippets, languages).
- Language dir `languages/<lang>/`: `config.toml` (`name`, `grammar`, `path_suffixes`,
  `line_comments`, `tab_size`, `first_line_pattern`, …) + queries `highlights`, `brackets`,
  **`outline`** (`@item @name @context @context.extra @annotation`), `indents`, `injections`,
  `overrides`, `textobjects`, `redactions`, `runnables`.
- Installed layout (`crates/extension_host/src/extension_host.rs`): `<extensions>/installed/<id>/`
  holds `extension.toml`, `languages/…`, `grammars/<grammar>.wasm`, `extension.wasm` (code only);
  `<extensions>/index.json` indexes them.
- Registry: Zed's API host (`api.zed.dev`) `GET /extensions/<id>/download` and
  `/extensions/<id>/<version>/download` with `min/max_schema_version` and
  `min/max_wasm_api_version`; body is `.tar.gz`. Not documented as a public API.
- WASM API: WIT `zed:extension` versions since_v0.0.1 … since_v0.8.0; stable Zed accepts up to
  0.6.x (`wasm_host/wit.rs:60-69`); v0.6.0 spans 0.6.0..0.7.0. Server launch is the extension's
  `language_server_command` → `Command { command, args, env }`; also
  `language_server_initialization_options`, `language_server_workspace_configuration`.
- Grammars are built by Zed from source with the wasi-sdk into `grammars/<name>.wasm`.


## Exploration 2 — node notes

Node-specific exploration is carried by the PRD's Background and the whole-work Exploration 1 above; the contract commit adds what its tests need.


## Exploration 3 — contract (2026-10-20)

- **WIT**: zed `main` at `c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f`, `crates/extension_api/wit/since_v0.6.0` — eleven
  `.wit` files (and a `settings.rs`, not vendored). Identical to the WIT inside `zed_extension_api` 0.6.0 except one
  doc comment in `dap.wit`.
- **wasmtime**: tree-sitter 0.26.13 → `wasmtime-c-api-impl` 36.0.19 → `wasmtime` 36.0.19 with `runtime`, `gc`,
  `std`, `cranelift`, `gc-drc` — no `component-model`. A direct `wasmtime = "36.0.19"` with `component-model`
  unifies to the same crate. wasmtime 36's `bindgen!` spells trappable imports `imports: { default: trappable }`.
- **One Engine is not possible in practice**: epoch interruption is an engine setting, and tree-sitter's `WasmStore`
  never sets an epoch deadline, so its store would trap at once on a shared engine.
- **Toolchain**: neither the dev shell's rustc 1.94.1 nor `~/.rustup` has `wasm32-wasip2`. The flake's pinned
  `rust-overlay` gives `rust-bin.stable."1.94.1".default.override { targets = ["wasm32-wasip2"]; }` (a Nix build,
  no repo change). Two clean builds produced the same bytes.
- **The component's imports** (from its name section): `wasi:cli/*`, `wasi:clocks/wall-clock`,
  `wasi:filesystem/*`, `wasi:io/*` at 0.2.0 and 0.2.6, plus `zed:extension/*`. `register_extension!`'s
  `init-extension` reads `PWD` and calls `set_current_dir`, so WASI is reached before any export.

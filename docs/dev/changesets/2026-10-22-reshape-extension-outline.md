# 2026-10-22 — an installed Zed extension's languages get anchors and outlines from its grammar (`#reshape` 20/24, #679)

**Type:** Feature

Packages: `tddy-zed-extensions`, `tddy-code-restructuring`, `tddy-index-daemon`, `tddy-tools`, `tddy-tools-cli`.
New dependency `tree-sitter` 0.26.13 with feature `wasm` (consented 2026-10-19), which brings wasmtime 36 through
`wasmtime-c-api` and re-exports it as `tree_sitter::wasmtime` — no second wasmtime is declared.

## `tddy-zed-extensions`

- `language.rs`: `ExtensionLanguage { extension, extension_dir, name, grammar, path_suffixes, outline_query }`,
  `load_all(dir, manifest)` (the manifest's `languages`, else every directory under `languages/`, sorted by name),
  `support_id()` (the name lower-cased, spaces → `-`), `grammar_wasm()`; `LanguageConfig` (the `config.toml`
  fields read).
- `grammar.rs`: `GrammarStore` — one wasmtime engine and one `WasmStore` per process, held inside its one
  `tree_sitter::Parser` (a WASM language parses only with the store that loaded it); `load` caches by
  `(extension dir, grammar)`; `GrammarLoadError { extension, grammar, reason }`, displayed
  `extension <id>: grammar <g> did not load: <reason>`.
- `outline.rs`: `Outline::of(tree, text, query)` — one `OutlineEntry { path, name, span, name_at }` per `@item`
  match with a `@name`; context = the names of enclosing `@item` nodes, outermost first; span = the `@item` node
  extended back over leading `@annotation`s, ending at its last non-whitespace byte. `entries_at`,
  `innermost_enclosing`.
- `support.rs`: `load_installed(dir)` reads `installed/*/extension.toml` (dot-directories skipped), starts no
  engine when no language is found, and returns `InstalledLanguages { supports, failures }`. A grammar that does not
  load fails every language on it and is reported once; an `outline.scm` that does not compile fails its own
  language only. `ExtensionSupport: LanguageSupport` (claims by extension or whole file name, no launch,
  `workspace_root` = the run's root, `backend(_, root)` = its `OutlineBackend` over `root`). `OutlineBackend:
  LanguageBackend + ItemResolver + ItemAtResolver` — `supports` is false for every kind; `module_of` answers
  `[<file>]`; fingerprints are `Fingerprint::of` the entry's whole lines, as Rust's. `InstalledExtensions` adds
  `SupportRegistry::with_installed_extensions(dir) -> Result<Vec<GrammarLoadError>, ExtensionError>`.
- `error.rs`: `ExtensionError::GrammarEngineUnavailable { message }` for a wasmtime engine that does not start.
- Fixtures: the tree-sitter-toml grammar (`tests/fixtures/grammars/tree-sitter-toml/toml.wasm`, 26,937 bytes, MIT,
  from Zed's `toml` 1.0.3 archive; `README.md` records source, commit and sha256), the `demo-toml` extension
  (suffixes `toml` and `rs`) and `broken-grammar` (two languages on a text `.wasm`).
- Tests: `tests/outline_acceptance.rs` (11), `language::tests` (4), `grammar::tests` (3), `support::tests` (1 — a
  broken `outline.scm` spares a sibling language on the same grammar).

## `tddy-code-restructuring`, `tddy-index-daemon`

See [packages/tddy-code-restructuring/docs/changesets/2026-10-22-extension-outline.md](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-22-extension-outline.md)
and [packages/tddy-index-daemon/docs/changesets/2026-10-22-extension-outline.md](../../../packages/tddy-index-daemon/docs/changesets/2026-10-22-extension-outline.md).

## `tddy-tools`, `tddy-tools-cli`

`index_client::run_restructure(args, installed)` takes a loader for the installed languages and calls it only on the
cold path; `tddy-tools-cli`'s `installed_languages()` registers them, printing each grammar failure once on stderr
(`restructure: extension <id>: grammar <g> did not load: …`).

## Cost

46 new lockfile packages. A standalone release binary loading a grammar and running a query is 11.2 MB (2 m 14 s clean
release compile). The debug `tddy-index-daemon` grew from 59.7 MB to 108.3 MB. Release sizes of `tddy-tools` and
`tddy-index-daemon` were not measured.

## Left open, by decision at `/pr-wrap`

| Item | Disposition |
|---|---|
| A whole-file-name suffix (`Cargo.lock`) is claimed but `BackendRegistry` routes by extension | backlog: `2026-10-22-extension-whole-file-name-suffix-is-not-routed` |
| The daemon's allow-list is Rust's alone | `TODO(extension-language-server)`, `#reshape` 22/24 |
| `snapshot_resolving` and `apply` do not route through `Options::installed` | `TODO(extension-language-server)`, `#reshape` 22/24 — an extension language has no operation to apply until it has a server |
| A failed WASM engine start reused `ExtensionError::Io` | fixed here: `GrammarEngineUnavailable` |
| Debug daemon binary +48.6 MB | recorded above; no action |

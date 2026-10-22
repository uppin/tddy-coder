# Changeset: an installed Zed extension's languages get anchors and outlines from its grammar

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (declarative Zed adapter, outline tier)
**Stack**: `#reshape` 20/24, branch `feature/reshape/extension-outline`, base `feature/reshape/extension-install`, green wave 2.
**PR**: [#679](https://github.com/uppin/tddy-coder/pull/679) (draft).

## Initial Discovery

[2026-10-19-reshape-extension-outline-initial-discovery.md](2026-10-19-reshape-extension-outline-initial-discovery.md)

## Affected Packages

- `packages/tddy-zed-extensions`: `language.rs`, `grammar.rs`, `outline.rs`, `support.rs` (new modules); depends on
  `tddy-code-restructuring` and `tddy-lsp`.
- `packages/tddy-code-restructuring`: the seams an extension language needs — `ItemResolver::module_of`, `runner::Options::installed`,
  `restructure_cli::run_with_installed` — and, at green, routing `anchors` / static `check` / `items_anchor` through
  them. It **cannot** call `with_installed_extensions` itself: `tddy-zed-extensions` implements its `LanguageSupport`,
  so the dependency runs the other way (plan correction, see below).
- `packages/tddy-index-daemon`: depends on `tddy-zed-extensions`; registration in `main.rs` (`installed_languages`);
  the installed-registry seam `WorkspaceIndex::with_supports` / `build_code_index_entry_with_supports`.
- `packages/tddy-tools` (green only): the cold CLI's registration — `index_client.rs` calls
  `restructure_cli::run_with_installed` with `SupportRegistry::new().with_installed_extensions(..)`. Not touched at the
  contract commit.
- Dependencies `tree-sitter` 0.26.13 (feature `wasm`) — consented 2026-10-19. `wasmtime` is **not** declared: the
  `wasm` feature brings wasmtime 36 through `wasmtime-c-api` and re-exports it as `tree_sitter::wasmtime`.

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-outline.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-outline.md)

## Summary

Each installed extension language becomes an `ExtensionSupport` (K11 interface) whose `OutlineBackend` answers item
anchors from tree-sitter + `outline.scm`, through the same item-anchor traits Rust's anchors use (`ItemResolver`,
`ItemAtResolver`, routed by `BackendRegistry`).

## Responsibility

| Symbol | Where |
|---|---|
| `ExtensionLanguage { extension, extension_dir, name, grammar, path_suffixes, outline_query }`, `load_all(dir, manifest)`, `support_id()` (R1), `grammar_wasm()`; `LanguageConfig` | `tddy-zed-extensions/src/language.rs` |
| `GrammarStore` (one engine + `WasmStore` per process, inside its `Parser`; `new`, `load(extension, dir, grammar)`, `parse`), `GrammarLoadError { extension, grammar, reason }` (R8 wording) | `tddy-zed-extensions/src/grammar.rs` |
| `Outline { entries }` (`of`, `entries_at`, `innermost_enclosing`) / `OutlineEntry { path, name, span, name_at }` (`segments`) | `tddy-zed-extensions/src/outline.rs` |
| `ExtensionSupport: LanguageSupport` (`new`, `language`, `outline_of`, `outline_backend(root)`; `backend(server, root)` builds the `OutlineBackend` over `root`), `OutlineBackend: LanguageBackend + ItemResolver + ItemAtResolver`, `InstalledLanguages`, `load_installed(dir)`, trait `InstalledExtensions` with `SupportRegistry::with_installed_extensions(dir) -> Result<Vec<GrammarLoadError>, ExtensionError>` | `tddy-zed-extensions/src/support.rs` |
| `ItemResolver::module_of(root, file)` (provided: Cargo's `module_path_of`) and its `BackendRegistry` routing | `tddy-code-restructuring/src/item_anchor.rs`, `registry.rs` |
| `runner::Options::installed: Arc<SupportRegistry>` (empty by default) | `tddy-code-restructuring/src/runner/options.rs` |
| `restructure_cli::run_with_installed(args, installed)` | `tddy-code-restructuring/src/restructure_cli.rs` |
| `installed_languages()` (the daemon's registration) | `tddy-index-daemon/src/main.rs` |
| `WorkspaceIndex::with_supports(supports: Arc<SupportRegistry>) -> Self` (the index's installed-registry seam; real body) | `tddy-index-daemon/src/index.rs` |
| `build_code_index_entry_with_supports(ports, supports: Arc<SupportRegistry>) -> ServiceEntry` (the host's entry over that registry; real body), re-exported from `lib.rs` | `tddy-index-daemon/src/service.rs` |

2026-10-20: K20 owns the daemon's installed-registry seam (`WorkspaceIndex::with_supports`,
`build_code_index_entry_with_supports`), moved down from K22 (developer decision). The daemon acceptance test builds
its host through it — the built-ins (Rust launching a nonexistent rust-analyzer), the installed extensions, the
allow-list from that registry — and asks `Anchors` in process, as a host that registers the service will.

### The rules (the contract)

- **R1 — id.** The support id is the language `name` lower-cased with spaces → `-` (`"Elixir"` → `elixir`).
- **R2 — claims.** `path_suffixes` only; registered after built-ins (K11 R4).
- **R3 — outline.** Matches of `outline.scm` over the parse tree; an entry = one `@item` match; `name` = `@name` text;
  path = the names of the entries whose `@item` node encloses this one, outermost first; span = the `@item` node
  extended over leading `@annotation` nodes, **ending at the node's last non-whitespace byte** (contract-stage
  refinement: a tree-sitter-toml `table` node runs on over the blank lines to the next table's line, column 0, which
  would put two adjacent tables on one line and fail `covering_run`'s adjacency test); one-based lines and byte
  columns, the end one past its last byte as `Range` is everywhere else; `name_at` = the start of `@name`.
- **R4 — item path.** `<file relative to the workspace root>::<context…>::<name>` (PRD F1 a): the extension backend's
  `ItemResolver::module_of` answers `[<file>]`, so `items_anchor`'s "bare name gets the file's prefix" rule and
  `segments_below` apply unchanged.
- **R5 — anchors (plan correction 2026-10-20).** `LanguageBackend::anchor_for` no longer exists —
  `anchors-outline` (#609, K16) deleted it (its R5). Extension anchors go through the **item-anchor traits** exactly as
  Rust's do: `restructure anchors --items` → `item_anchor::items_anchor` → `ItemResolver::resolve_item` per name +
  `covering_run`; `--at` → `item_anchor_at` → `ItemAtResolver::item_enclosing`; a plan's `item` / `items` anchors →
  `resolve_item_anchors` / `span_of`. All three route by the `BackendRegistry` to the `OutlineBackend`. The fingerprint
  is `Fingerprint::of` the entry's whole lines (Rust's); non-adjacent entries are refused by the shared
  `covering_run`, so in Rust's words. Green makes `items_anchor` read the prefix through `ItemResolver::module_of`
  instead of `module_path_of`. An `ItemResolver` gets the file relative to the root and not the root, and an
  extension backend has no server to learn it from, so the `OutlineBackend` holds the root it was built over by
  K11's `LanguageSupport::backend(server, root)`.
- **R6 — static check.** A stale fingerprint is refused as for Rust: `RestructureError::ItemChanged { item, file }`.
  A static `check` resolves the item anchors of files a server-less support claims through `Options::installed`'s
  backends (cheap — no server) instead of giving them `anchors-outline`'s "only a deep check can resolve" finding or
  its Cargo prefix rule (`refuse_a_foreign_prefix`, which would refuse a file outside any package).
- **R7 — no server.** `supports(kind)` is false for every kind (outline only); `backend_for` then yields
  `UnsupportedOp { backend: "<id>", op }`.
- **R8 — load failure.** A grammar that fails to load: one report per run naming extension, grammar and reason
  (`extension <id>: grammar <g> did not load: <reason>`, one per grammar however many languages share it); those
  languages claim nothing; every other extension still loads.
- **R9 — what is loaded.** `load_installed` reads `installed/*/extension.toml` (the installed layout, not
  `index.json`), skipping dot-directories (a failed install's `.<id>.partial`); no `installed/` is no extensions.
  Languages: the manifest's `languages`, else every directory under `languages/`, sorted by name.

## Boundaries

- No language server (K22); no code actions (K23); no WASM beyond the grammar (K24).
- Queries read: `outline.scm` only (PRD F3 a).

## Dependencies

- `feature/reshape/language-support` (#675, K11) — **real edge**: `LanguageSupport`, `SupportRegistry`, `LanguageId`.
- `feature/reshape/extension-install` (K19) — reuses its `ExtensionsDir` (`under_data_dir`, `installed`, `extension`),
  `ExtensionManifest` and `ExtensionError` (`LanguageConfigMissing`, `ManifestUnparsable`) as published. **New real
  edge (contract stage):** production registration needs `ExtensionsDir::resolve()` (K19, `todo!`), so the daemon test
  `the_daemon_answers_the_same_anchors_as_the_cli` and the CLI wiring wait on K19 green. The library suite does not:
  its fixtures write the installed layout directly and reach no K19 function body. K19 is wave 1, so the wave order
  holds.
- `feature/reshape/anchors-outline` (#609, K16) — **interaction, not a surface:** K16 deleted `anchor_for` (hence R5's
  correction) and its R2/R3 make the static `check` apply the Cargo prefix rule to every item anchor. R6 needs
  extension-language anchors kept out of that rule and resolved instead; green does that in `check_plan` before
  `item_anchor_findings`, without editing K16's `refuse_a_foreign_prefix` / `item_anchor_findings`. K16 is wave 1.
- This PR does NOT: change `LanguageSupport` / `ServerHandle` / `SupportRegistry` (K11), `install_*` / `Index` /
  `ExtensionsDir::resolve` (K19), or `refuse_a_foreign_prefix` / `item_anchor_findings` (K16).

### Seams this node adds to `tddy-code-restructuring`, and why

- ~~**`bind_workspace`**~~ — removed, see the note below: the root now reaches a backend through K11.
- **`ItemResolver::module_of`** — `items_anchor` prefixes bare names with `module_path_of` (Cargo). Provided method,
  default = that, so Rust is unchanged.
- **`Options::installed` / `run_with_installed`** — the plan put the registration call in `restructure_cli.rs` /
  `runner/entry_points.rs`, which is a dependency cycle (`tddy-zed-extensions` → `tddy-code-restructuring`). The front
  ends (`tddy-tools`, `tddy-index-daemon`) build the registry and hand it in.

2026-10-20: the workspace root reaches a backend through K11's `LanguageSupport::backend(server, root)`;
`bind_workspace` removed.

## Draft PR contract

- Surface: the four modules plus the `tddy-code-restructuring` seams above, every body `todo!()`
  (`TODO(reshape-extension-outline)`). The registration calls are declared (`run_with_installed`,
  `installed_languages`) but **not yet called**: calling them would reach `todo!()` (and K19's `resolve`) on every
  existing CLI and daemon run. Green wires them.
- Fixture grammar (PRD F2 a, chosen 2026-10-20): **tree-sitter-toml**, `tests/fixtures/grammars/tree-sitter-toml/`
  — `toml.wasm` (26,937 bytes, sha256 `de6823f0a8722d857e097a98b514ca98e04a45db1138991ab29f0eeaec72b86a`, grammar ABI
  13), MIT (Copyright (c) Ika, `LICENSE` from the grammar repository at `342d9be207c2dba869b9967124c679b5e6fd0ebe`),
  taken from Zed's published `toml` 1.0.3 extension archive (wasi-sdk build by Zed); `README.md` records source URL,
  commit and hashes. Verified to load and parse with `tree-sitter` 0.26.13 `wasm`. Zed's own query files (Apache-2.0)
  are not vendored.
- Fixture extensions: `demo-toml` (language `Demo TOML`, id `demo-toml`, suffixes `toml` and `rs`, its own
  `outline.scm`; the suites copy the vendored grammar in as `grammars/toml.wasm`, so the binary is committed once) and
  `broken-grammar` (two languages on one grammar whose `.wasm` is text). K19's `demo-lang` is untouched.
- Failing tests (`packages/tddy-zed-extensions/tests/outline_acceptance.rs`): `an_extension_language_claims_its_suffixes`,
  `a_builtin_keeps_a_suffix_an_extension_also_claims`, `the_outline_lists_each_item_with_its_context`,
  `anchors_print_an_item_path_and_fingerprint_per_entry`, `an_at_anchor_names_the_innermost_entry_with_its_context`,
  `an_items_anchor_over_adjacent_entries_resolves`, `non_adjacent_entries_are_refused`,
  `check_reports_a_stale_fingerprint`, `a_rename_is_unsupported_without_a_server`,
  `a_broken_grammar_is_reported_once_and_spares_other_languages`, `a_grammar_load_failure_says_which_extension_and_grammar`;
  `packages/tddy-index-daemon/tests/extension_anchors.rs`: `the_daemon_answers_the_same_anchors_as_the_cli` (in process, through
  `build_code_index_entry_with_supports`; red on `SupportRegistry::with_installed_extensions`' `todo!()`).
- Unit tests: `tddy-zed-extensions` `language::tests` (4: R1 id, languages dir scan, manifest-listed languages,
  missing config), `grammar::tests` (3: vendored grammar loads and parses, loaded once per store, missing file names
  extension and grammar); `tddy-code-restructuring` `registry::tests` (2: `module_of` routes to the claiming backend,
  which answers from the root it was built over; `module_of` through a backend without a resolver is refused by name).

## Green wave

**Wave:** 2 of 4 (unchanged by the contract's tests).
**Greenable independently:** no — needs `language-support` (K11) green (`a_builtin_keeps_a_suffix…` drives
`with_builtins` / `support_of`; the runner builds its registries through `SupportRegistry`), and for the daemon test and
the production wiring `extension-install` (K19) green (`ExtensionsDir::resolve`). Both are wave 1. Every other library
test reaches only this node's bodies plus code already implemented.
**Concurrent with:** `backend-fn-sizes` (K21).
**Blocks:** `extension-language-server` (K22) and, through it, K23, K24.

## Successor PRs

`feature/reshape/backend-fn-sizes` (K21).

## Scope

- [x] Contract: surface published with `todo!()` bodies; builds, `clippy -D warnings` clean (scoped)
- [x] Contract: failing acceptance tests written, each verified failing on this node's own `todo!()` (the daemon test
  on the daemon's missing routing)
- [x] Contract: failing unit tests for the owned surface
- [ ] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [x] Language loading, grammar store, outline
- [x] Support + backend + registration (CLI and daemon)
- [x] Fixture grammar vendored with licence (tree-sitter-toml, MIT)
- [ ] Docs: `## Languages from an extension` in zed-extensions.md; `## Languages` in rust-code-restructuring.md
- [x] Binary-size/compile-time note for wasmtime in the changeset
- [ ] Code Quality

## Testing plan

Library level with a real grammar WASM fixture; one daemon test. No language server.

## Dependency cost (wasmtime)

Measured at the contract commit, `tree-sitter` 0.26.13 `wasm` on this Mac (Apple Silicon, rustc 1.94.1):

- **Lockfile:** 46 new packages (cranelift, wasmtime 36 internals, wasmparser, `wasmtime-c-api-impl`, …).
- **Clean compile:** a standalone binary with only `tree-sitter` + `wasm` took **2 m 14 s** in release from an empty
  target dir.
- **Binary size:** that standalone release binary (load a grammar, parse, run a query) is **11.2 MB**. The real
  growth of `tddy-tools` / `tddy-index-daemon` is measured at green, once the code is reachable; at the contract
  commit the daemon's debug binary grew 0.8 MB (58.9 → 59.7 MB) because nothing calls into wasmtime yet.
- **Daemon, measured at green:** the debug `tddy-index-daemon` is **108.3 MB** (59.7 MB at the contract commit, +48.6 MB)
  once the grammar store is reachable from `main`. Release size of the daemon and of `tddy-tools` is **not measured**
  (the standalone 11.2 MB figure above is the estimate).

## Refactoring Needed

- `snapshot_resolving` and `apply` (`store_run`) do not route through `Options::installed`: a plan with extension-language
  item anchors is not re-resolved by `snapshot`, and an apply of one is refused for want of a rust-analyzer session.
  Extension languages support no operation (R7), so `apply` has nothing to do; `snapshot` of such a plan is a follow-up.
- A whole-file-name `path_suffixes` entry (`Cargo.lock`) is claimed by `ExtensionSupport::claims` but
  `BackendRegistry` routes by extension, so it never reaches the outline backend (TODO in `support.rs`).
- The daemon's allow-list is the built-in Rust one only; K22 extends it when an installed language launches a server
  (TODO(extension-language-server) in `tddy-index-daemon/src/main.rs`).

## Documentation notes (packages/*/docs is not edited directly)

- `## Languages from an extension` for the zed-extensions package doc: suffix claims (R2), the outline tier (R3), item path
  `<file>::<context>::<name>` (R4), no server (R7), a grammar that does not load reported once per run (R8).
- `## Languages` for the rust-code-restructuring package doc: `Options::installed`, `run_with_installed`,
  `ItemResolver::module_of`, `BackendRegistry::extend`, and the static `check` resolving server-less item anchors (R6).

## Validation Results

Scoped, through `./dev`:

- `./test -p tddy-zed-extensions`: 10 unit + cli 3 + install 15 + outline 11, all pass.
- `./test -p tddy-code-restructuring -p tddy-index-daemon`: 2032 passed, 0 failed, 19 ignored (incl. `the_daemon_answers_the_same_anchors_as_the_cli`).
- `cargo clippy -p tddy-zed-extensions -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools -p tddy-tools-cli --all-targets -- -D warnings`: clean.
- `tddy-tools` / `tddy-tools-cli` compile (`--all-targets`); their test suites were not run.

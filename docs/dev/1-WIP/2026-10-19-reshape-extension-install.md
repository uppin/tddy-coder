# Changeset: `tddy-tools extensions` installs Zed extensions into `~/.tddy/extensions`

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (extension installation)
**Complexity**: 4/5 — new crate + tddy-tools, HTTP/archive I/O seam
**Stack**: `#reshape` 19/24, branch `feature/reshape/extension-install`, base `feature/reshape/fn-sizes-rest`, green wave 1.
**PR**: [#678](https://github.com/uppin/tddy-coder/pull/678) (draft)

## Initial Discovery

[2026-10-19-reshape-extension-install-initial-discovery.md](2026-10-19-reshape-extension-install-initial-discovery.md)

## Affected Packages

- `packages/tddy-zed-extensions` (new crate): manifest, extensions dir, install/list/remove, clap args.
- `packages/tddy-tools`: `Subcommand::Extensions`.
- Root `Cargo.toml` workspace members; `Cargo.lock` (`tar` added).

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-install.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-install.md)
- New: `docs/ft/coder/zed-extensions.md` (written at wrap from the PRD), linked from `docs/ft/coder/1-OVERVIEW.md`.

## Summary

Install, list and remove Zed extensions under `<tddy data dir>/extensions`, in Zed's installed layout, from Zed's
registry or a local directory. Nothing loads them yet.

## Responsibility

| Symbol | Where |
|---|---|
| crate `tddy-zed-extensions` | `packages/tddy-zed-extensions` |
| `ExtensionsDir` — `under_data_dir(data_dir)`, `resolve()` (env, CLI only), `resolve_from(tddy_data_dir, home)` (`TDDY_DATA_DIR` else `$HOME/.tddy`, then `/extensions`), `root()`, `installed()`, `extension(id)`, `index()` | `src/dir.rs` |
| `ExtensionManifest` (+ `GrammarEntry`, `LanguageServerEntry`, `LibEntry`, `LibKind`), `parse`, `check_schema_version`, `SUPPORTED_SCHEMA_VERSIONS` | `src/manifest.rs` |
| `ExtensionError` — one variant per R3/R4 refusal, `file`/`entry` relative to the extension root | `src/error.rs` |
| `Registry` — `at(api)`, `from_env()` (`TDDY_ZED_EXTENSIONS_API` else `DEFAULT_API`, CLI only), `api()` | `src/registry.rs` |
| `install_from_registry(dir, registry, id, version)` (async), `install_from_path(dir, path)`, `list(dir)`, `remove(dir, id)`, `InstallOutcome { Installed, Replaced, AlreadyInstalled }`, `InstalledExtension` + `line()` (R6) | `src/install.rs` |
| `Index { extensions }` + `load`/`save`, `IndexEntry { id, version, source, installed_at }` (Unix seconds), `ExtensionSource { Registry, Path }` (`index.json`) | `src/index.rs` |
| `ExtensionsArgs` / `ExtensionsCommand { Install, List, Remove }`, `InstallArgs`, `RemoveArgs`, `run(args)` | `src/cli.rs` |
| `Subcommand::Extensions(ExtensionsArgs)` | `packages/tddy-tools/src/main.rs` |

Library functions take the directory and the registry **explicitly**; only `cli::run` reads the environment
(`TDDY_DATA_DIR`, `HOME`, `TDDY_ZED_EXTENSIONS_API`), so tests never race on process env. That is why
`install_from_registry` takes a `Registry` the wave-1 table did not list.

### The rules (the contract)

- **R1 — layout.** `<dir>/installed/<id>/` holds the extension exactly as unpacked; `<dir>/index.json` the entries.
- **R2 — atomic install.** Unpack to `<dir>/installed/.<id>.partial`, validate, rename over `installed/<id>`; on any
  refusal the partial dir is removed and the previous install is untouched.
- **R3 — validation** (each names the extension and the file): no or unparsable `extension.toml`; `id` ≠ requested;
  `schema_version` ∉ 0..=1; an absolute, `..`-climbing or outward-symlink archive entry; a declared grammar without
  `grammars/<name>.wasm`; a `languages/<x>/` without `config.toml`.
- **R4 — registry.** `GET {api}/extensions/{id}/download` or `/{id}/{version}/download` with `min_schema_version=0`,
  `max_schema_version=1`, `min_wasm_api_version=0.6.0`, `max_wasm_api_version=0.7.0` (Zed's own `since_v0_6_0` range) (PRD F1 a); `api` =
  `TDDY_ZED_EXTENSIONS_API` or `https://api.zed.dev`. A non-200 is refused with the status and the URL.
- **R5 — re-install.** Same version: no-op, said so; different version: replaced (PRD F3 a).
- **R6 — list** prints `<id> <version> <source> — languages: …; grammars: …; language servers: …; wasm: yes|no`.

## Boundaries

- Does **not** register languages, load grammars or run WASM (K20, K24).
- Does **not** build grammars or extensions.

## Dependencies

- `feature/reshape/fn-sizes-rest` (#613, K18) — no real edge (line position).

## Draft PR contract

Published in commit 2 (the first push of this PR, not its deliverable):

- **Surface**: the crate with every install/list/remove/index/dir-resolution body `todo!()`
  (`TODO(reshape-extension-install)`); the manifest types, `ExtensionError`, `Registry`, the path accessors and the clap
  args real; the `tddy-tools` arm wired. Dependencies: `tar` (new, consented), `flate2`, `toml` 0.9, `reqwest` 0.12
  (`rustls-tls`, no default features — as `tddy-tools`), `thiserror` 2; dev: `wiremock` 0.6 (already `tddy-tools`'
  fixture HTTP server — no new crate), `tokio`, `tempfile`, `pretty_assertions`.
- **Fixture** `tests/fixtures/demo-lang/`: `extension.toml` (id `demo-lang`, 0.1.0, schema 1, grammar `demo`),
  `languages/demo/config.toml` (`Demo`, `path_suffixes = ["demo"]`), and `grammars/demo.wasm` — a **placeholder**: the
  8-byte empty WASM module header. This node only checks the file is present; `extension-outline` (K20) vendors a
  real grammar.
- **Test support** `tests/support/mod.rs`: `a_data_dir()` (temp data dir + `ExtensionsDir`), `the_demo_lang_archive()`
  (builds the `.tar.gz` from the fixture — PRD F2 a — with `with_version`, `with_id`, `without`, and `with_raw_entry`,
  which writes a `..` name into the tar header verbatim), `a_fixture_registry()` (wiremock answering only
  `/extensions/<id>[/<version>]/download` with the four R4 query parameters).
- **Failing tests** — `packages/tddy-zed-extensions/tests/install_acceptance.rs`:
  `installs_from_the_registry_into_the_installed_layout`, `records_the_install_in_the_index`,
  `lists_what_an_extension_provides`, `installs_a_dev_extension_from_a_path`, `refuses_an_archive_entry_that_climbs_out`,
  `refuses_a_grammar_the_archive_does_not_ship`, `refuses_a_manifest_whose_id_differs`,
  `a_refused_install_keeps_the_previous_version`, `reinstalling_the_same_version_changes_nothing`,
  `reinstalling_a_different_version_replaces_it` (added: PRD F3 a's other half),
  `removes_an_extension_and_its_index_entry`, `removing_an_unknown_id_names_it`, `everything_happens_under_tddy_data_dir`.
  - unit (`src/manifest.rs`): `schema_version_outside_the_range_is_refused` (red);
    `manifest_parses_zeds_fields` is **green on purpose** — the manifest types are plain serde structs, real in the
    contract.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes — every test drives only this crate, against a temp data dir and a local
fixture registry; no parent surface is reached (the contract's tests found no new edge).
**Concurrent with:** every other wave-1 node (K11–K18).
**Blocks:** none (K20's fixtures write the installed layout directly).

## Successor PRs

`feature/reshape/extension-outline` (K20).

## Scope

- [x] Create failing acceptance tests (contract commit)
- [x] Run acceptance tests (verify they fail) — 13 acceptance + 1 unit fail on this node's `todo!()`
- [x] TDD Red — write failing unit/integration tests
- [x] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [x] Crate, manifest, dir, index
- [x] Registry and path install (R2–R5), list, remove
- [x] `tddy-tools extensions`
- [ ] Docs: `docs/ft/coder/zed-extensions.md` (install section)
- [ ] Code Quality

## Testing plan

Library-level acceptance against a local HTTP server and temp `TDDY_DATA_DIR`; no network, no WASM.

## Refactoring Needed

- `src/install.rs` is 470 lines, past the ~400-line guideline and under the 500 `pr-wrap` line. Staging,
  `copy_tree` and `remove_tree` are the seams for a private module if `pr-wrap` wants it below 400.
- Open at green: `list` fails for every extension when one installed `config.toml` does not parse, because
  validation only checks that the file exists. A chain of in-tree symlinks that resolves outside is caught only
  lexically. Both are for review, not this green.

## Validation Results

Contract commit (2026-10-20), scoped to the touched packages:

- `cargo clippy -p tddy-zed-extensions --all-targets -- -D warnings` — clean.
- `cargo clippy -p tddy-tools --all-targets -- -D warnings` — clean.
- `cargo fmt --all --check` — clean.
- `cargo test -p tddy-zed-extensions --no-fail-fast` — 1 passed (`manifest_parses_zeds_fields`), 14 failed, every one a
  `todo!()` panic in this crate (`install.rs`, `dir.rs`, `manifest.rs`).
- `cargo test -p tddy-tools --no-fail-fast` — all pass.

Green (2026-10-20), scoped to the touched packages, through the nix dev shell (`./dev cargo …`):

- `cargo test -p tddy-zed-extensions --no-fail-fast` — 16 passed (13 acceptance, 3 unit), 0 failed.
- `cargo clippy -p tddy-zed-extensions --all-targets -- -D warnings` — clean.
- `cargo clippy -p tddy-tools --all-targets -- -D warnings` — clean.
- `cargo test -p tddy-tools --no-fail-fast` — all pass.
- `cargo fmt -p tddy-zed-extensions -p tddy-tools -- --check` — clean.

## 2026-10-20: `LanguageServerEntry` reads Zed's opt-in and action-kind fields

Developer decision on K22's question: `LanguageServerEntry` also parses Zed's `opt_in_languages`
(`BTreeSet<String>`) and `code_action_kinds` (`Option<Vec<String>>`), beside the `language_ids` it already
read — the three fields of Zed's `LanguageServerManifestEntry` that K22 (server choice, `languageId`) and
K23 (code actions) need. Plain serde fields; pinned green by
`manifest::tests::a_language_server_entry_reads_its_opt_in_languages_ids_and_action_kinds`.

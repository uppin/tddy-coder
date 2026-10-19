# `tddy-tools extensions` installs Zed extensions into `~/.tddy/extensions` - PRD

**Date**: 2026-10-19
**PRD Type**: New feature (extension installation)
**Stack**: `#reshape` 19/24 (`feature/reshape/extension-install`, base `feature/reshape/fn-sizes-rest`; no real parent —
wave 1; nothing waits on it: the adapter nodes read an installed layout their fixtures write directly)

## Affected Features

- **New feature document**: `docs/ft/coder/zed-extensions.md` — what a Zed extension gives tddy, where it is
  installed, and the `extensions` CLI. Linked from `docs/ft/coder/1-OVERVIEW.md`.

## Summary

`tddy-tools extensions` manages Zed editor extensions for tddy:

- `install <id>[@<version>]` downloads the extension's archive from Zed's extension registry and unpacks it into
  `<extensions dir>/installed/<id>/`;
- `install --path <dir>` copies a local extension directory (a dev extension) the same way;
- `list` prints each installed extension: id, version, source, and what it provides (languages, grammars, language
  servers, whether it has `extension.wasm`);
- `remove <id>` deletes it.

The **extensions directory** is `<tddy data dir>/extensions`, the data dir resolved as the daemon resolves it:
`TDDY_DATA_DIR`, else `~/.tddy` (per user — developer decision 2026-10-19). The layout under it is Zed's own
(`installed/<id>/{extension.toml, languages/…, grammars/<g>.wasm, extension.wasm?}`), so a directory Zed installed can be
copied in unchanged.

## Background

- Zed's host downloads `GET <api>/extensions/<id>/download` (latest compatible) or `/extensions/<id>/<version>/download`
  with `min/max_schema_version` and `min/max_wasm_api_version` query parameters; the body is a `.tar.gz` holding the
  built extension (grammars already compiled to WASM). `api.zed.dev` is Zed's own client API, not documented as public.
- tddy builds nothing: an extension whose `extension.toml` declares a grammar must ship `grammars/<grammar>.wasm`.

## Proposed Changes

### What's Changing

- **New crate `tddy-zed-extensions`** (`packages/tddy-zed-extensions`): `ExtensionsDir`, `ExtensionManifest` (parsed
  from `extension.toml` with `toml`: `id, name, version, schema_version, description?, repository?, authors, lib,
  languages, grammars, language_servers`), `install_from_registry`, `install_from_path`, `list`, `remove`, and
  `ExtensionsArgs` (clap) — the house pattern of `restructure_args.rs` / `analyze_cli.rs`.
- **`tddy-tools`**: a `Subcommand::Extensions(ExtensionsArgs)` arm (`packages/tddy-tools/src/main.rs:32–99`).
- **Install is atomic**: unpack into `installed/.<id>.partial`, validate, then rename over `installed/<id>`; a failed
  install leaves the previous version in place.
- **Validation, each refusal naming the extension and the file**: missing or unparsable `extension.toml`; manifest
  `id` differing from the requested id; a `schema_version` outside 0..=1; an archive entry that is absolute or climbs
  out (`..`), or a symlink leaving the directory; a declared grammar without `grammars/<name>.wasm`; a language dir
  without `config.toml`.
- **Index**: `<extensions dir>/index.json` records `{ id, version, source: registry|path, installed_at }` per extension.
- **Registry endpoint**: `https://api.zed.dev`, overridable by `TDDY_ZED_EXTENSIONS_API` (configuration, also how the
  tests point at a local fixture server).
- **Dependencies (consented 2026-10-19)**: `tar` (new); `flate2`, `toml`, `reqwest` (already in the workspace).

### What's Staying the Same

- Nothing loads an installed extension yet; `extension-outline` (K20) does.
- No WASM is run, no grammar is built, nothing is executed from the archive.

## Impact Analysis

- New crate; `tddy-tools` gains one subcommand arm. `./install` and `./release` need no change (no new binary).
- Network: only on `install <id>`.

## Acceptance Criteria

- [ ] `install demo-lang@0.1.0` against a fixture registry unpacks into `installed/demo-lang/`, writes `index.json`, and
      `list` prints `demo-lang 0.1.0 registry — languages: Demo; grammars: demo; language servers: none; wasm: no`.
- [ ] `install --path <dir>` copies a dev extension and records `source: path`.
- [ ] An archive with a `../` entry, a missing grammar WASM, or a mismatched `id` is refused, and the previously
      installed version is still present.
- [ ] `remove demo-lang` deletes the directory and the index entry; removing an unknown id is an error naming it.
- [ ] With `TDDY_DATA_DIR=<tmp>` everything happens under `<tmp>/extensions`.

## Open questions

- **F1 — WASM API range requested from the registry.** (a) Request the range `extension-wasm-command` (K24) will run
  (recommended: 0.6.x), so a downloaded code extension is runnable; (b) request any version — declarative parts work
  regardless.
- **F2 — fixture source for tests.** (a) A small test archive built by the test from a vendored grammar WASM
  (recommended; see `extension-outline` F2); (b) record a real registry download.
- **F3 — `install` of an id already installed.** (a) Replace when the version differs, no-op when equal
  (recommended); (b) require `--force`.

## References

- Zed sources: `crates/extension/src/extension_manifest.rs`, `crates/extension_host/src/extension_host.rs`
  (`/extensions/<id>/download`, installed layout, `index.json`).
- Discovery: `docs/dev/1-WIP/2026-10-19-zed-extensions-whole-work-initial-discovery.md` §10 (data dir), §11 (deps).

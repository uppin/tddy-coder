# Changeset: `tddy-tools extensions` installs Zed extensions into `~/.tddy/extensions`

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (extension installation)
**Stack**: `#reshape` 19/24, branch `feature/reshape/extension-install`, base `feature/reshape/fn-sizes-rest`, green wave 1.

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
| `ExtensionsDir` (`resolve()`: `TDDY_DATA_DIR` else `$HOME/.tddy`, then `/extensions`; `installed()`, `index()`) | `src/dir.rs` |
| `ExtensionManifest` (+ `GrammarEntry`, `LanguageServerEntry`, `LibEntry`) parsed from `extension.toml` | `src/manifest.rs` |
| `install_from_registry(dir, id, version)`, `install_from_path(dir, path)`, `list(dir)`, `remove(dir, id)` | `src/install.rs` |
| `Index` / `IndexEntry { id, version, source, installed_at }` (`index.json`) | `src/index.rs` |
| `ExtensionsArgs` / `ExtensionsCommand { Install, List, Remove }` | `src/cli.rs` |
| `Subcommand::Extensions(ExtensionsArgs)` | `packages/tddy-tools/src/main.rs` |

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

- Surface: the crate with every public function `todo!()` (`TODO(reshape-extension-install)`), the manifest types
  real (they are plain serde structs), the clap args and the `tddy-tools` arm wired.
- Failing tests (`packages/tddy-zed-extensions/tests/install_acceptance.rs`, a local HTTP fixture registry serving a
  `.tar.gz` built by the test from `tests/fixtures/demo-lang/` — PRD F2 a):
  `installs_from_the_registry_into_the_installed_layout`, `records_the_install_in_the_index`,
  `lists_what_an_extension_provides`, `installs_a_dev_extension_from_a_path`, `refuses_an_archive_entry_that_climbs_out`,
  `refuses_a_grammar_the_archive_does_not_ship`, `refuses_a_manifest_whose_id_differs`,
  `a_refused_install_keeps_the_previous_version`, `reinstalling_the_same_version_changes_nothing`,
  `removes_an_extension_and_its_index_entry`, `removing_an_unknown_id_names_it`,
  `everything_happens_under_tddy_data_dir`.
  - unit: `manifest_parses_zeds_fields`, `schema_version_outside_the_range_is_refused`.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes.
**Concurrent with:** every other wave-1 node (K11–K18).
**Blocks:** none (K20's fixtures write the installed layout directly).

## Successor PRs

`feature/reshape/extension-outline` (K20).

## Scope

- [ ] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [ ] Crate, manifest, dir, index
- [ ] Registry and path install (R2–R5), list, remove
- [ ] `tddy-tools extensions`
- [ ] Docs: `docs/ft/coder/zed-extensions.md` (install section)
- [ ] Code Quality

## Testing plan

Library-level acceptance against a local HTTP server and temp `TDDY_DATA_DIR`; no network, no WASM.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._

# 2026-10-22 — an extension language's whole-file-name suffix is claimed but never routed

**Category:** Restructure tooling gap
**Found in:** `#reshape` 20/24 (`extension-outline`, PR #679), at `/green`
**Owner:** unassigned — not K22–K24's scope (they add a server, code actions and WASM commands, not routing)

## What happens

Zed's `config.toml` `path_suffixes` holds file extensions (`toml`) and whole file names (`Cargo.lock`, `Dockerfile`).
`ExtensionSupport::claims` (`packages/tddy-zed-extensions/src/support.rs`) honours both, so
`Options::is_served_without_a_server` says such a file needs no server. But `BackendRegistry::backend_of`
(`packages/tddy-code-restructuring/src/registry.rs`) routes by `Path::extension()` alone, through
`LanguageBackend::handles_extension(&str)`. A file named by a whole-name suffix never reaches the `OutlineBackend`:
`restructure anchors` on it fails with "no backend", never with the wrong answer.

## Why it was not fixed in #679

The fix is in the routing interface K11 (`language-support`) owns, not in the extension node: `handles_extension` takes
an extension, and every backend implements it. Passing the file name through the same method would blur what it
means for the Rust backend. No fixture language of #679 claims a whole file name.

## Wanted

`BackendRegistry` routes a file by the support that claims its path — the same question `SupportRegistry::support_of`
already answers — or `LanguageBackend` gets a `handles_path(&Path)` with `handles_extension` as its default, so a
whole-name suffix reaches the outline backend. Then a test in `tddy-zed-extensions/tests/outline_acceptance.rs` with a
fixture language claiming a whole file name.

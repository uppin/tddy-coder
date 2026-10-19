# Changeset: an installed Zed extension's languages get anchors and outlines from its grammar

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (declarative Zed adapter, outline tier)
**Stack**: `#reshape` 20/24, branch `feature/reshape/extension-outline`, base `feature/reshape/extension-install`, green wave 2.

## Initial Discovery

[2026-10-19-reshape-extension-outline-initial-discovery.md](2026-10-19-reshape-extension-outline-initial-discovery.md)

## Affected Packages

- `packages/tddy-zed-extensions`: `language.rs`, `grammar.rs`, `outline.rs`, `support.rs` (new modules).
- `packages/tddy-code-restructuring`: `SupportRegistry::with_installed_extensions` call sites
  (`restructure_cli.rs`, `runner/entry_points.rs`).
- `packages/tddy-index-daemon`: the same registration in `main.rs`.
- Dependencies `tree-sitter` (feature `wasm`), `wasmtime` (consented 2026-10-19).

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-outline.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-outline.md)

## Summary

Each installed extension language becomes an `ExtensionSupport` (K11 interface) whose `OutlineBackend` answers item
anchors and `anchor_for` from tree-sitter + `outline.scm`.

## Responsibility

| Symbol | Where |
|---|---|
| `ExtensionLanguage { name, grammar, path_suffixes, outline_query }` loaded from `languages/<x>/` | `src/language.rs` |
| `GrammarStore` (one `WasmStore` per process; `load(dir, grammar)`) | `src/grammar.rs` |
| `Outline` / `OutlineEntry { path: Vec<String>, name, span, name_at }` from `outline.scm` | `src/outline.rs` |
| `ExtensionSupport: LanguageSupport`, `OutlineBackend: LanguageBackend + ItemResolver + ItemAtResolver` | `src/support.rs` |
| `SupportRegistry::with_installed_extensions(dir)` (extension method in `tddy-zed-extensions`, called by both entry points) | `src/support.rs` |

### The rules (the contract)

- **R1 — id.** The support id is the language `name` lower-cased with spaces → `-` (`"Elixir"` → `elixir`).
- **R2 — claims.** `path_suffixes` only; registered after built-ins (K11 R4).
- **R3 — outline.** Matches of `outline.scm` over the parse tree; an entry = one `@item` match; `name` = `@name` text;
  path = the `@context` names of enclosing entries, outermost first; span = the `@item` node extended over leading
  `@annotation` nodes; `name_at` = the start of `@name`.
- **R4 — item path.** `<file relative to the workspace root>::<context…>::<name>` (PRD F1 a).
- **R5 — anchors.** `restructure anchors [--items]` emits one anchor per entry with item path + the same sha256
  fingerprint Rust uses; `anchor_for(file, items)` returns the union span of adjacent entries and refuses non-adjacent
  ones with Rust's message shape.
- **R6 — static check.** A stale fingerprint is reported as for Rust.
- **R7 — no server.** `supports(kind)` is false for every kind (outline only); `backend_for` then yields
  `UnsupportedOp { backend: "<id>", op }`.
- **R8 — load failure.** A grammar that fails to load: one report per run naming extension, grammar and reason; that
  extension's languages claim nothing.

## Boundaries

- No language server (K22); no code actions (K23); no WASM beyond the grammar (K24).
- Queries read: `outline.scm` only (PRD F3 a).

## Dependencies

- `feature/reshape/language-support` (#675, K11) — **real edge**: `LanguageSupport`, `SupportRegistry`, `LanguageId`.
- `feature/reshape/extension-install` (K19) — line position; reuses its `ExtensionsDir` and `ExtensionManifest`
  (surface published at K19's contract commit).

## Draft PR contract

- Surface: the four modules with `todo!()` bodies (`TODO(reshape-extension-outline)`), the registration calls wired.
- Fixture: `tests/fixtures/grammars/<small MIT grammar>.wasm` + `LICENSE` (PRD F2 a — the grammar is chosen at the
  contract commit, ≤200 KB, built with the wasi-sdk; the choice and its licence go in this changeset) and a fixture
  extension `demo-lang` using it with an `outline.scm`.
- Failing tests (`packages/tddy-zed-extensions/tests/outline_acceptance.rs`): `an_extension_language_claims_its_suffixes`,
  `a_builtin_keeps_a_suffix_an_extension_also_claims`, `the_outline_lists_each_item_with_its_context`,
  `anchors_print_an_item_path_and_fingerprint_per_entry`, `an_items_anchor_over_adjacent_entries_resolves`,
  `non_adjacent_entries_are_refused`, `check_reports_a_stale_fingerprint`, `a_rename_is_unsupported_without_a_server`,
  `a_broken_grammar_is_reported_once_and_spares_other_languages`;
  `packages/tddy-index-daemon/tests/extension_anchors.rs`: `the_daemon_answers_the_same_anchors_as_the_cli`.

## Green wave

**Wave:** 2 of 4.
**Greenable independently:** no — needs `language-support` (K11) green (the registry must route by support).
**Concurrent with:** `backend-fn-sizes` (K21).
**Blocks:** `extension-language-server` (K22) and, through it, K23, K24.

## Successor PRs

`feature/reshape/backend-fn-sizes` (K21).

## Scope

- [ ] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [ ] Language loading, grammar store, outline
- [ ] Support + backend + registration (CLI and daemon)
- [ ] Fixture grammar vendored with licence
- [ ] Docs: `## Languages from an extension` in zed-extensions.md; `## Languages` in rust-code-restructuring.md
- [ ] Binary-size/compile-time note for wasmtime in the changeset
- [ ] Code Quality

## Testing plan

Library level with a real grammar WASM fixture; one daemon test. No language server.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._

# Languages are supports behind one tddy interface; built-in Rust is the first - PRD

**Date**: 2026-10-19
**PRD Type**: Enhancement (an open language interface) + behaviour-preserving restructure (Rust moves behind it)
**Stack**: `#reshape` 11/24 (`feature/reshape/language-support`, base `feature/reshape/move-item-paths`; no real
parent among the open nodes — wave 1, leads it: `extension-outline`, `extension-language-server`,
`extension-code-actions`, `extension-wasm-command` and `backend-fn-sizes` wait on it)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — a new section `## Languages`
  describing the language-support interface and that Rust is its built-in implementation.
- [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md) — its server allow-list comes from the same
  supports (no behaviour change).

## Summary

1. **One open language id.** `tddy_lsp::LanguageId` (an interned string, `LanguageId::RUST = "rust"`) replaces both
   `enum Language { Rust }` — `tddy_lsp::Language` (`allowlist.rs:11`) and `tddy_code_restructuring::registry::Language`
   (`registry.rs:16`). `LspKey`, `LspAllowList` and `LanguageBackend::language()` take it.
2. **A language support** is what a language brings to tddy, in one place:
   - its id and the LSP `languageId` it opens documents as;
   - which files it claims (path suffixes);
   - how its server starts (`LaunchSpec`) and how the server's workspace root is found from a file;
   - the backend that resolves its operations (`Box<dyn LanguageBackend>`), built over that server.
3. **`SupportRegistry`** holds the supports. From it come the `LspAllowList` (replacing the two hand-built production
   allow-lists, `restructure_cli.rs:257` and `tddy-index-daemon/src/main.rs:208`) and the `BackendRegistry`
   (replacing `registry_for_static` / `registry_for_waiting`'s direct `RustBackend` construction).
4. **`RustSupport`** is the built-in implementation: `.rs`, `rust-analyzer` with today's capabilities and settings, the
   Cargo workspace-root rule, `RustBackend`. Every Rust behaviour is unchanged.
5. The hard-coded `"languageId": "rust"` (`backends/rust/documents.rs:40`, `tddy-lsp/src/registry.rs:356`) comes from
   the support.

## Background

- The goal of the replanned stack (`plans/reshape/zed-extensions-replan.md`, 2026-10-19): per-language support becomes
  a tddy-defined interface that both built-in languages and imported Zed extensions implement. Built-ins stay
  built-in (developer decision), so this node gives the interface its first, real implementation.
- Today Rust is wired in at five places: two `Language` enums, two production allow-list builders, two registry
  builders, two `languageId` literals and a Cargo-only workspace root (`tddy-lsp/src/registry.rs:311`). Every later node
  that adds a language would otherwise repeat that wiring.
- The extension seam is the one the tests need first: `extension-outline` (K20) registers one support per installed
  extension language; `extension-language-server` (K22) gives such a support a `LaunchSpec`.

## Proposed Changes

### What's Changing

- **`tddy_lsp::LanguageId`** (`allowlist.rs`): `#[derive(Clone, PartialEq, Eq, Hash, Debug)] pub struct LanguageId(Arc<str>)`,
  `LanguageId::new(&str)`, `LanguageId::RUST`, `as_str()`, `Display`. `Language` is removed; `id()` callers use `as_str()`.
  `language_for_target_type` returns `Option<LanguageId>`.
- **`LaunchSpec`** gains `language_id: String` (the LSP `languageId`, default from the support's id) and the document
  sync in `tddy-lsp/src/registry.rs:356` reads it.
- **`LanguageSupport`** (new, `tddy-code-restructuring/src/language.rs`):
  ```rust
  pub trait LanguageSupport: Send + Sync {
      fn id(&self) -> &LanguageId;
      fn claims(&self, path: &Path) -> bool;
      fn launch(&self) -> Option<LaunchSpec>;          // None: no server (outline-only support)
      fn workspace_root(&self, file: &Path, root: &Path) -> PathBuf;
      fn backend(&self, server: ServerHandle<'_>) -> Box<dyn LanguageBackend>;
  }
  ```
  `ServerHandle` carries what `RustBackend::from_lsp_client` takes today (client, cancel, progress, trace, heartbeat),
  or nothing for the static tier.
- **`SupportRegistry`**: `with_builtins()` (Rust only today), `register(Box<dyn LanguageSupport>)`, `support_of(path)`,
  `allow_list() -> LspAllowList`, `backends(..) -> BackendRegistry`. The first support that claims a file wins, and
  built-ins are registered first.
- **`BackendRegistry::backend_of`** keeps matching by the backend; `registry::Language` and its `{:?}` error text are
  replaced by the id (`UnsupportedOp { backend: "rust", .. }` — the same text as today's `Rust` lower-cased; see F3).
- Callers switched: `restructure_cli.rs`, `runner/entry_points.rs`, `tddy-index-daemon` (`main.rs`, `index.rs` LspKeys,
  `apply.rs`, `queries.rs`, `plan_upkeep.rs`), and the `Language::Rust` uses in tests (~50, mechanical).

### What's Staying the Same

- Every restructure operation, refusal, note, CLI line and wire message.
- `LspAllowList::rust_only()` and its callers in `tddy-daemon`, `tddy-sandbox-app`, `tddy-lsp-executor` (they launch a
  bare rust-analyzer and do not use the restructure capabilities); they only switch to `LanguageId::RUST`.
- The index daemon's Rust-only navigation (`navigation.rs`, `Cargo.toml` manifest rule) — extension languages reach the
  daemon in `extension-language-server`.
- `RustBackend`'s construction surface and `client_capabilities` / `server_settings`.

## Impact Analysis

### Technical Impact

- Packages: `tddy-lsp`, `tddy-code-restructuring`, `tddy-index-daemon`, and test-only edits in `tddy-lsp-executor`,
  `tddy-daemon`, `tddy-sandbox-app` (enum → id).
- No new dependency.
- Wide but mechanical in tests; the production change is concentrated in `allowlist.rs`, `registry.rs` (both crates),
  `entry_points.rs`, `restructure_cli.rs` and the daemon's `main.rs`.

### User Impact

- None visible. An error that named the backend `Rust` names it `rust` (F3).

## Implementation Plan

1. `LanguageId` in tddy-lsp; `LaunchSpec.language_id`; `Language` removed (mechanical, engine `rename_symbol` where it
   applies).
2. `LanguageSupport` + `SupportRegistry` + `RustSupport`; allow-list and backend registry built from it.
3. Switch the two allow-list builders and two registry builders; delete the duplicates.
4. Tests: unit (claims, first-wins, allow-list equality with today's, `languageId` from the spec), acceptance (CLI cold
   path and daemon warm path still resolve a Rust plan, the allow-list the daemon builds equals the CLI's).

## Acceptance Criteria

- [ ] `tddy_lsp::Language` and `registry::Language` no longer exist; `LanguageId::RUST.as_str() == "rust"`.
- [ ] `SupportRegistry::with_builtins().allow_list()` yields the same `LaunchSpec` for rust as today's
      `restructure_allow_list` and the daemon's `rust_analyzer_as_this_crate_needs_it` (one builder left).
- [ ] A support that claims `.rs` registered after `RustSupport` does not take `.rs` files.
- [ ] A document opened through the registry carries the support's `languageId`.
- [ ] The existing `tddy-code-restructuring`, `tddy-lsp` and `tddy-index-daemon` suites pass unchanged except the
      enum → id edit.

## Open questions (decided at PRD review)

- **F1 — where the interface lives.** (a) `tddy-code-restructuring::language` (recommended: the backend trait is
  there, and the crate split is stack 2); (b) a new low crate `tddy-language` both `tddy-code-restructuring` and the
  Zed adapter depend on.
- **F2 — precedence when a built-in and an extension claim the same suffix.** (a) built-in wins, always
  (recommended; a Zed Rust-like extension never replaces `RustSupport`); (b) an explicit setting picks.
- **F3 — error text** `UnsupportedOp { backend: "Rust" }` becomes `"rust"`. (a) accept the lower-case id
  (recommended); (b) keep a display name on the support.

## References

### Affected Features (Complete List)

- `docs/ft/coder/rust-code-restructuring.md`, `docs/ft/coder/warm-code-intelligence-daemon.md`

### Related Documentation

- `plans/reshape/zed-extensions-replan.md`; `docs/dev/1-WIP/2026-10-19-zed-extensions-whole-work-initial-discovery.md`
- `packages/tddy-lsp/docs/workspace-root.md` (the Cargo rule becomes `RustSupport::workspace_root`)

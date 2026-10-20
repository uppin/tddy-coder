# Changeset: an extension language's server starts from tddy settings, and gives it rename and references

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (declarative language-server launch for extension languages)
**Stack**: `#reshape` 22/24, branch `feature/reshape/extension-language-server`, base `feature/reshape/backend-fn-sizes`, green wave 3.
**PR**: [#681](https://github.com/uppin/tddy-coder/pull/681)

## Initial Discovery

[2026-10-19-reshape-extension-language-server-initial-discovery.md](2026-10-19-reshape-extension-language-server-initial-discovery.md)

## Affected Packages

- `packages/tddy-zed-extensions`: `settings.rs`, `ExtensionSupport::launch`.
- `packages/tddy-lsp`: `workspace/configuration` answered from the spec's settings (`client/dispatch.rs:106`).
- `packages/tddy-code-restructuring`: `ServerBackend` (generic rename), composed with K20's `OutlineBackend`.
- `packages/tddy-index-daemon`: code-index queries route by support (`navigation.rs`), index state for
  extension-only worktrees.
- Docs that say non-Rust worktrees get no index: `packages/tddy-index-daemon/docs/code-index-service.md`,
  `packages/tddy-web/docs/session-start-and-indexing-progress.md`.

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-language-server.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-language-server.md)

## Summary

`<extensions dir>/settings.json` in Zed's `lsp` schema gives an extension language's server a command; with it the
language has `rename_symbol` and references, on the CLI and through the index daemon.

## Responsibility

| Symbol | Where |
|---|---|
| `ExtensionSettings { lsp }` (`file`, `parse`, `load`, `server`), `ServerSettings { binary, initialization_options, settings }`, `BinarySettings { path, arguments, env }`, `SETTINGS_FILE` | `tddy-zed-extensions/src/settings.rs` |
| `ServerChoice { Undeclared, Unconfigured(NoServerCommand), Configured { server, launch } }` (`for_language` = R1–R4, `launch`), `NoServerCommand` (R3 wording), `client_capabilities()` | `tddy-zed-extensions/src/server.rs` |
| private field `ExtensionSupport::server: ServerChoice` + `with_server`, `server()`; the green-time behaviour of K20's `launch()`, `backend()` and `load_installed` (see the `impl ExtensionSupport` doc there) | `tddy-zed-extensions/src/support.rs` |
| `LaunchSpec::workspace_configuration: Value` + `with_workspace_configuration` (real) | `tddy-lsp/src/allowlist.rs` |
| `configuration_answer(settings, params)` — the `workspace/configuration` reply from the spec's settings | `tddy-lsp/src/client/dispatch.rs` |
| `fake_lsp` modes `--requests-configuration <section>` (+ `tddy/configurationAnswers`) and `--answers-rename` | `tddy-lsp/tests/bin/fake_lsp.rs` |
| `ServerBackend` (`over(outline, live, language_id)`, `without_command(outline, reason)`; `rename_symbol` via `textDocument/rename`) | `tddy-code-restructuring/src/backends/server.rs` |
| `RestructureError::ServerNotConfigured { language, reason }` (mapped `failed_precondition` in the daemon) | `tddy-code-restructuring/src/lib.rs`, `tddy-index-daemon/src/status.rs` |
| `WorkspaceIndex::with_supports` (real), `WorkspaceIndex::client_for_file` (routing), `build_code_index_entry_with_supports` (real) | `tddy-index-daemon/src/index.rs`, `service.rs` |
| `serves_workspace(root, supports)` — the predicate callers ask instead of `is_rust_workspace` | `tddy-index-daemon/src/lib.rs` |

### The rules (the contract)

- **R1 — which server.** The manifest's `[language_servers.<id>]` whose `language(s)` names the language; several →
  the first not `opt_in_languages` for it. **Open (contract stage):** K19's `LanguageServerEntry` has no
  `opt_in_languages` field, and this node may not add one — see *Open questions*; the multi-server tie-break has no test.
- **R2 — command.** Only from `lsp.<id>.binary.path` (+ `arguments`, `env`). No PATH lookup (PRD F1 a).
- **R3 — missing command.** Any server-needing op is refused before a process starts:
  `language server <id> (extension <ext>) has no command: set lsp.<id>.binary.path in <file>`.
- **R4 — configuration.** `initialization_options` sent at `initialize`; `workspace/configuration` items answered from
  `settings` by their `section` path (null where absent); Rust's spec carries none (unchanged).
- **R5 — root.** The restructure / worktree root (PRD F2 a).
- **R6 — rename.** `rename_symbol` on an item anchor sends `textDocument/rename` at the entry's `name_at`; the returned
  `WorkspaceEdit` is the resolution.
- **R7 — daemon.** definition / references / hover for an extension-language file go to its server; an extension-only
  worktree whose language has a server reports an index state other than `NONE`.

## Boundaries

- No WASM (K24); code actions and verify are K23.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**; implementing one here
collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `extension-outline` (K20) — **real edge** | `ExtensionSupport` (`new`, `outline_backend`, `LanguageSupport` impl), `OutlineBackend`, `load_installed`, `InstalledExtensions::with_installed_extensions`, `ResolvedItem.name` via `OutlineBackend`'s `ItemResolver` | every acceptance test loads supports through `load_installed` / `with_installed_extensions`; `ServerBackend` composes the outline backend for item anchors and the rename position | change their signatures. It adds one **private field** (`server`) to `ExtensionSupport` — K20's `new` sets it to `ServerChoice::default()` — and green changes the bodies of K20's `launch`, `backend` and `load_installed` as the `impl ExtensionSupport` doc in `support.rs` says |
| `language-support` (K11) | `LanguageSupport`, `ServerHandle`/`LiveServer`, `SupportRegistry::{allow_list, with_builtins}`, `LaunchSpec::language_id` | the daemon test builds its allow-list with `allow_list()`; backends are built through `LanguageSupport::backend` | change `LaunchSpec`'s existing fields (it adds `workspace_configuration` only) |
| `extension-install` (K19) | `ExtensionsDir`, `ExtensionManifest::parse`, `LanguageServerEntry { language, languages, language_ids }`, `ExtensionError::Io` | `ServerChoice::for_language` reads the manifest's server entries; `ExtensionSettings::load` refuses with `ExtensionError::Io` | add `opt_in_languages` to `LanguageServerEntry` (open question) |
| `anchors-outline` (K16) | `ServerNotStarted` refusals (its red tests in `tddy-lsp` / `tddy-index-daemon` fail on this branch until K16 is green) | nothing | — |
| `backend-fn-sizes` (K21) | the 60-line gate over `backends/` | `backends/server.rs` must stay under it | — |

**Sequencing:** the acceptance tests reach K20's `load_installed` / `with_installed_extensions` (`todo!`) before any
body of this node, and the daemon test K11's `allow_list` too; the unit tests reach only this node's bodies.

## Draft PR contract

- Surface: the symbols above, bodies `todo!()` (`TODO(reshape-extension-language-server)`); real: the `LaunchSpec`
  field and builder, the settings structs' serde shape, `ExtensionSettings::file`, `ServerChoice::launch`,
  `NoServerCommand`'s message, `ExtensionSupport::{with_server, server}`, `WorkspaceIndex::with_supports`,
  `build_code_index_entry_with_supports`. Nothing calls the new routing yet: `configuration_answer` and
  `client_for_file` are unwired (wiring them now would reach `todo!` on every Rust session).
- Fixtures: `demo-toml` (K20's) with `[language_servers.fake-ls] languages = ["Demo TOML"]` appended to the installed
  manifest by the suites, and `settings.json` pointing `lsp.fake-ls.binary.path` at `fake_lsp` (now also a `[[bin]]`
  of `tddy-zed-extensions`; `tddy-task` a dev-dependency there).
- Acceptance tests:
  `tddy-zed-extensions/tests/language_server_acceptance.rs`: `a_configured_server_renames_an_extension_symbol`,
  `the_server_receives_the_initialization_options`, `workspace_configuration_is_answered_from_settings`,
  `an_unconfigured_server_is_refused_before_anything_starts`;
  `tddy-index-daemon/tests/extension_references.rs`: `the_daemon_answers_references_for_an_extension_file`,
  `an_extension_only_worktree_has_an_index_state`;
  `tddy-lsp/tests/workspace_configuration_test.rs`: `the_servers_configuration_request_is_answered_from_the_launch_settings`
  (and `a_launch_without_settings_answers_null`, green on purpose: Rust's unchanged behaviour).
  `an_opt_in_language_does_not_pick_the_server` is **not written** — R1's open question.
- Unit tests: `tddy-zed-extensions` `settings::tests` (4), `server::tests` (5; `the_refusal_reads_as_the_prd_words_it`
  green on purpose — it pins R3's wording, which is data); `tddy-lsp` `client::dispatch::tests` (4 new);
  `tddy-code-restructuring` `backends::server::tests` (3); `tddy-index-daemon` lib `tests` (3, `serves_workspace`
  against stand-in supports).

## Green wave

**Wave:** 3 of 4 (unchanged by the contract's tests).
**Greenable independently:** no — needs `extension-outline` (K20) green (every acceptance test loads supports through
`load_installed`), and `language-support` (K11) for the daemon test's `allow_list`; both are earlier waves.
**Concurrent with:** none.
**Blocks:** `extension-code-actions` (K23), `extension-wasm-command` (K24).

## Successor PRs

`feature/reshape/extension-code-actions` (K23).

## Open questions

- **R1 tie-break.** `opt_in_languages` is not a field of K19's `LanguageServerEntry` (nor verified in Zed's manifest);
  adding it is K19's surface. Until decided, `ServerChoice::for_language` documents the multi-server case as open.
- **Daemon seam.** K20's daemon anchors need the installed registry inside the service as much as R7 does; this node
  publishes `WorkspaceIndex::with_supports` / `build_code_index_entry_with_supports`. K20's green should use them
  rather than add a second seam.
- **Index state outside the daemon.** `CodeIndexState::None` is decided in `tddy-daemon-rpc` (and
  `tddy-lsp-executor`) by `is_rust_workspace`; R7 needs those callers to ask `serves_workspace` with the host's
  registry — packages outside this node's list.
- **State of an extension server.** `index_state_of` reads rust-analyzer's `experimental/serverStatus`; an extension
  server never sends it, so its answers read `LOADING`. The references test asserts locations only.
- **Position encoding.** `client_capabilities()` asks for `utf-8`; a server that settles on `utf-16` must be converted
  or refused by `ServerBackend` (TOML fixtures are ASCII, so no test pins it).

## Scope

- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (batched; ticked at `/green` start)
- [x] TDD Red — write failing unit/integration tests
- [ ] Settings + launch spec
- [ ] `workspace/configuration` from settings
- [ ] Generic rename
- [ ] Daemon routing + index state
- [ ] Docs: `## Language servers` in zed-extensions.md; index-daemon and tddy-web docs corrected
- [ ] Code Quality

## Testing plan

Library and daemon level against the fake LSP; no real extension server.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._

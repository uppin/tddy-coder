# Changeset: languages are supports behind one tddy interface; built-in Rust is the first

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: Enhancement (open language interface) + behaviour-preserving restructure (Rust moves behind it)
**Stack**: `#reshape` 11/24, branch `feature/reshape/language-support`, base `feature/reshape/move-item-paths`, green wave 1.
**PR**: [#675](https://github.com/uppin/tddy-coder/pull/675) (draft)

## Initial Discovery

[2026-10-19-reshape-language-support-initial-discovery.md](2026-10-19-reshape-language-support-initial-discovery.md)

## Prerequisites

No claimed code issue in the path. Backlog entries this node runs into:

- [2026-10-17-restructuring-acceptance-suite-pays-a-rust-analyzer-start-per-test.md](../todo/2026-10-17-restructuring-acceptance-suite-pays-a-rust-analyzer-start-per-test.md)
  — *during*: the "no fake `LanguageBackend` until it has a consumer" rule (2026-10-17) still holds; this node adds no
  fake support outside `#[cfg(test)]` unit tests of the registry itself.
- [2026-09-16-backends-rust-rs-is-4500-production-lines.md](../todo/2026-09-16-backends-rust-rs-is-4500-production-lines.md)
  — *unrelated to the fix*: this node moves construction out of `rust.rs` (a few dozen lines) but does not split it.

## Affected Packages

- `packages/tddy-lsp` — `LanguageId`, `LaunchSpec.language_id`, `LspKey`, document sync `languageId`.
- `packages/tddy-code-restructuring` — `language.rs` (new), `registry.rs`, `runner/entry_points.rs`,
  `restructure_cli.rs`, `backends/rust/documents.rs`, `backends/rust.rs` (construction only).
- `packages/tddy-index-daemon` — allow-list and registry from `SupportRegistry`; `LspKey` ids.
- Test-only enum→id edits: `tddy-lsp-executor`, `tddy-daemon`, `tddy-sandbox-app`.

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-language-support.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-language-support.md)
- [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md), [warm-code-intelligence-daemon.md](../../ft/coder/warm-code-intelligence-daemon.md)

## Summary

One open `LanguageId`; a `LanguageSupport` trait bundling a language's file claims, server launch, workspace-root rule
and backend; a `SupportRegistry` that builds the LSP allow-list and the backend registry; `RustSupport` as the only
built-in. No behaviour change for Rust.

## Background

See the PRD. The replanned stack (`#reshape` 11–24) makes per-language support an interface shared by built-ins and
Zed extensions; this node is the interface and its first real implementation.

## Responsibility

This PR owns:

| Symbol | Where | Notes |
|---|---|---|
| `LanguageId` (`new`, `RUST`, `as_str`, `Display`) | `tddy-lsp/src/allowlist.rs` | replaces `tddy_lsp::Language` |
| `LaunchSpec::language_id` + builder `with_language_id` | `tddy-lsp/src/allowlist.rs` | default = the id's string |
| `LanguageSupport` trait, `ServerHandle` | `tddy-code-restructuring/src/language.rs` | F1 (a) |
| `SupportRegistry` (`with_builtins`, `register`, `support_of`, `allow_list`, `backends`, `backends_static`) | `language.rs` | first claim wins; built-ins first (F2 a) |
| `RustSupport` | `tddy-code-restructuring/src/backends/rust/support.rs` | wraps today's construction |
| `LanguageBackend::language() -> &LanguageId` | `registry.rs` | `registry::Language` removed |

### The rules (the contract)

- **R1 — one id.** No `enum Language` remains in `tddy-lsp` or `tddy-code-restructuring`. `LanguageId::RUST.as_str()`
  is `"rust"`. `language_for_target_type` returns `Option<LanguageId>`.
- **R2 — one allow-list builder.** The production allow-list with capabilities and settings is built only by
  `SupportRegistry::allow_list()`. `restructure_cli.rs::restructure_allow_list` and
  `tddy-index-daemon::rust_analyzer_as_this_crate_needs_it` are deleted (callers switched). `LspAllowList::rust_only()`
  stays for its three external callers.
- **R3 — one registry builder.** `registry_for_static` / `registry_for_waiting` build through
  `SupportRegistry::backends_static` / `backends(..)`; no `RustBackend::new` or `from_lsp_client` call remains outside
  `RustSupport`.
- **R4 — claims.** `support_of(path)` returns the first registered support whose `claims` is true; built-ins are
  registered first, so an extension never takes `.rs` (PRD F2 a).
- **R5 — languageId.** The document-sync path (`tddy-lsp/src/registry.rs:356`, `backends/rust/documents.rs:40`) sends
  the spec's `language_id`.
- **R6 — error text.** `UnsupportedOp.backend` / `NoBackend` carry the id string (`"rust"`), PRD F3 (a).

## Boundaries

- Does **not** load, list or know of Zed extensions (K19, K20).
- Does **not** make the index daemon serve non-Rust files (K22): its `navigation.rs` constants stay.
- Does **not** add operations, change `SUPPORTED`, or split `rust.rs`.
- Does **not** touch `LspAllowList::rust_only()` callers beyond the enum→id edit.

## Dependencies

- `feature/reshape/move-item-paths` (#608, K10, WIP) — **no real edge**: the base by line position only. Textual overlap
  in `backends/rust.rs` and `registry.rs` is expected at rebase.
- Merged `#reshape` 1–9 — the current `RustBackend` construction surface, `with_silence_bounds` (node 9).

## Draft PR contract

Published as this PR's second commit (wave 2):

- Surface: `LanguageId`, `LaunchSpec::language_id`, `LanguageSupport`, `ServerHandle`, `SupportRegistry`, `RustSupport`,
  each body `todo!()` with `TODO(reshape-language-support)` where it cannot be a pass-through; the enum→id rename done
  for real (it is mechanical and every later node compiles against the id).
- Failing tests:
  - `language.rs` unit: `claims_route_a_file_to_the_first_support_that_takes_it`,
    `a_later_support_never_takes_a_suffix_a_builtin_claims`, `the_allow_list_carries_each_supports_launch`.
  - `tests/language_support_acceptance.rs` (library level, no rust-analyzer):
    `the_rust_launch_the_registry_builds_equals_the_one_the_cli_used_to_build`,
    `the_daemon_and_the_cli_build_the_same_allow_list`, `a_document_opens_with_its_supports_language_id`,
    `an_unsupported_operation_names_the_language_by_its_id`.
  - Existing suites stay green except where the enum is spelled (edited in the same commit).

### As published (wave 2, 2026-10-19)

Real in this commit (mechanical, every later node compiles against it):

- `tddy_lsp::LanguageId(Cow<'static, str>)` — a `Cow`, not the PRD's `Arc<str>`, because `LanguageId::RUST` must be
  a `const`. `LspAllowList::{is_allowed, launch_spec}` take `&LanguageId`; `allow` takes it by value.
- `LaunchSpec::language_id` + `with_language_id`; `LspAllowList::allow` fills an empty `language_id` with the id's
  string ("default = the id's string"). Nothing reads it yet — R5's two literals are green's.
- `LanguageBackend::language() -> &LanguageId`; `registry::Language` removed. R6 is done: every `UnsupportedOp.backend`
  is `language().to_string()` (`"rust"`), including `RustBackend::resolve_opening`'s literal; the one existing unit test
  that spelled `"Rust"` now spells `"rust"`.
- `SupportRegistry::{new, with_builtins, with_builtin_rust, register}`, `ServerHandle::{none, live, server}`,
  `LiveServer` (client, cancel, progress, `trace: Option<fn(&str)>`, wait_heartbeat), `RustSupport::{new, launching}`,
  `RustSupport::id`.
- The fake server replays `tddy/documentLanguages` (`{ uri: languageId }`), the observable for R5.

`todo!()` (`TODO(reshape-language-support)`): `SupportRegistry::{support_of, allow_list, backends, backends_static}`,
`RustSupport::{claims, launch, workspace_root, backend}`. Builders untouched (`restructure_allow_list`,
`rust_analyzer_as_this_crate_needs_it`, `registry_for_static/waiting`, `srcs_to_document_sources`'s `"rust"`): switching
and deleting them is green's (R2, R3, R5).

`with_builtin_rust(RustSupport::launching(program))` is the daemon's constructor: its rust-analyzer path is configured.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes — every test builds a registry from `RustSupport` or a `#[cfg(test)]` support.
**Concurrent with:** `move-impl-members` (#610), `tests-follow` (#611), `move-widen` (#604), `detach-method`,
`anchors-outline` (#609), `oversized-files` (#612), `fn-sizes-rest` (#613), `extension-install`.
**Blocks:** `extension-outline` (K20), `backend-fn-sizes` (K21) and, through K20, K22–K24.

Real dependency edges (open nodes): `11→20`, `11→21`, `12→21`, `13→21`, `14→21`, `15→21`, `20→22`, `22→23`, `22→24`.

## Successor PRs

`feature/reshape/move-impl-members` (#610, K12).

## Scope

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

- [ ] USER REVIEW of the acceptance tests (ticked at `/green` start)
- [x] Draft PR contract: surface + failing tests (wave 2) ✅
- [x] `LanguageId` + enum removal (tddy-lsp, all callers) ✅
- [~] `LaunchSpec::language_id` and the document-sync path (field + default done; the two literals remain)
- [~] `LanguageSupport`, `SupportRegistry`, `RustSupport` (surface; bodies `todo!()`)
- [ ] Allow-list and registry builders switched; duplicates deleted
- [ ] Docs: `## Languages` in rust-code-restructuring.md; workspace-root.md names `RustSupport`
- [ ] Code Quality (`/analyze-clean-code` B or better)

## Technical changes

### State A (Current)

Two `enum Language { Rust }`; allow-list built at `restructure_cli.rs:257` and `tddy-index-daemon/src/main.rs:208`;
registries at `runner/entry_points.rs:93,125`; `"rust"` literals at `documents.rs:40`, `tddy-lsp/src/registry.rs:356`;
Cargo workspace root at `tddy-lsp/src/registry.rs:311`.

### State B (Target)

`LanguageId`; `SupportRegistry::with_builtins()` → `RustSupport`; allow-list, registries and `languageId` read from it.

### Delta (What's Changing)

See Responsibility R1–R6.

## Implementation milestones

1. `LanguageId` + `LaunchSpec::language_id` (tddy-lsp), all `Language::Rust` call sites → `LanguageId::RUST`.
2. `language.rs` + `RustSupport`.
3. Builders switched, duplicates deleted.
4. Docs.

## Testing plan

### Testing Strategy

Library level only; no new rust-analyzer test (2026-10-17 rule: no new live suite where library level proves it). The
existing live suites prove Rust is unchanged.

**Locations**: `packages/tddy-code-restructuring/src/language.rs` (unit), `packages/tddy-code-restructuring/tests/language_support_acceptance.rs`.

## Refactoring Needed

_None yet._

## Validation Results

Wave 2 contract (scoped: tddy-lsp, tddy-code-restructuring, tddy-index-daemon, tddy-lsp-executor): `cargo check
--all-targets` and `cargo clippy --all-targets -- -D warnings` clean. Red: the 3 `language.rs` unit tests and the 4
`language_support_acceptance` tests fail on `todo!` in this node's own surface; every other `tddy-code-restructuring`
lib test (1093), all of `tddy-lsp`, `tddy-lsp-executor --lib`, `tddy-index-daemon --lib --bins` and the five fake-server
restructuring suites pass.

# Changeset: `apply_code_action` runs any language server's code action by name, and extension runs are verified

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (generic code-action operation; verify for extension languages)
**Stack**: `#reshape` 23/24, branch `feature/reshape/extension-code-actions`, base `feature/reshape/extension-language-server`, green wave 4.
**PR**: [#682](https://github.com/uppin/tddy-coder/pull/682)

## Initial Discovery

[2026-10-19-reshape-extension-code-actions-initial-discovery.md](2026-10-19-reshape-extension-code-actions-initial-discovery.md)

## Affected Packages

- `packages/tddy-lsp`: `workspace/executeCommand` request; server→client `workspace/applyEdit` delivered to the
  waiting request instead of `null` (`client/dispatch.rs:106`).
- `packages/tddy-code-restructuring`: `RefactorKind::ApplyCodeAction`, `RefactorOp.action`, codec + static check,
  `ServerBackend` and `RustBackend` dispatch, `runner/comparison.rs` verify by support.
- `packages/tddy-zed-extensions`: the extension-language verify (diagnostics + parse).
- `packages/tddy-index-daemon`: `status.rs` maps `CodeActionRefused` (exhaustive match). **Widened
  2026-10-23:** `Verify` builds `Options::verifies` and answers `VerifyResponse.source_findings` (proto field 9).
- `packages/tddy-tools` / `packages/tddy-tools-cli`: **widened 2026-10-23** — the cold `restructure verify` builds
  `Options::verifies` and renders `SourceFinding`s; the warm client renders `source_findings`.
- `.config/nextest.toml`, `.config/rust-e2e.filterset`: the live `apply_code_action` suite.

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-code-actions.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-code-actions.md)

## Summary

A generic `apply_code_action` operation (Rust included, PRD F2 a) and, for extension languages, a verify of "no new
error diagnostics and no parse `ERROR` node" (PRD F1 a).

## Responsibility

| Symbol | Where |
|---|---|
| `RefactorKind::ApplyCodeAction`; `RefactorOp::action: Option<ActionSelector>`, `ActionSelector { title, kind }` (both optional in the shape, R4 refuses both/neither) | `plan/refactor_kind.rs`, `plan.rs` |
| R4 plan-line refusals (`rules`) | `plan/codec/code_action_fields.rs` |
| `CodeActionServer` (the four requests), `select` (R1), `resolve(server, op, range, only, workspace)` (R1–R2), `OfferedAction`, `CodeActionRefusal { NotOffered, Ambiguous, NoEdit }` (+ `Display`), `impl CodeActionServer for LspClientBridge` | `tddy-code-restructuring/src/backends/code_action.rs` |
| `RestructureError::CodeActionRefused(CodeActionRefusal)` (mapped `failed_precondition` in the daemon) | `tddy-code-restructuring/src/lib.rs`, `tddy-index-daemon/src/status.rs` |
| `RustBackend::apply_code_action` and its `resolve_opening` arm; `SUPPORTED` 28 → 29 | `backends/rust/code_action_op.rs`, `backends/rust.rs` |
| `SourceVerify`, `SourceChange`, `SourceFinding { NewErrorDiagnostic, ParseError }`, `ParseNode` — the verify seam | `tddy-code-restructuring/src/verify/source.rs` |
| `Options::verifies`, `runner::verify_sources(root, &options)` (dispatch by the verify that claims a file) | `runner/options.rs`, `runner/comparison.rs` |
| `LspClient::execute_command` → `ExecutedCommand { result, applied_edits }`; `ApplyEditCapture`, `apply_edit_answer` | `tddy-lsp/src/client/commands.rs` |
| `fake_lsp` modes `--code-actions <file>` (+ `tddy/applyEditAnswers`, `tddy/codeActionRequests`) and `--diagnoses <marker>` | `tddy-lsp/tests/bin/fake_lsp.rs` |
| `ExtensionVerify` (`new`, `impl SourceVerify`), `new_error_diagnostics`, `parse_errors` | `tddy-zed-extensions/src/verify.rs` |
| `ExtensionSupport::parse` (a K23 `impl` block beside K20's, since it reads the support's private grammar and store) | `tddy-zed-extensions/src/support.rs` |

### The rules (the contract)

- **R1 — selection.** `textDocument/codeAction` over the anchor's range; `title` = exact match, `kind` = prefix match on
  the action's kind; 0 matches → refused, listing every offered title + kind; >1 → refused, listing the matches.
- **R2 — resolution.** `codeAction/resolve` when the action has no edit and the server supports resolve; an `edit` is
  the resolution; else its `command` is sent with `workspace/executeCommand` and every `workspace/applyEdit` the server
  sends **during that call** is captured (answered `{applied: true}`), merged in arrival order, and is the resolution;
  a command that yields no edit is refused.
- **R3 — nothing is written by the server.** The captured edit goes through the normal apply path (journal, compile
  gate for Rust).
- **R4 — static check.** `action` present with exactly one of `title` / `kind`; anchor is a range or items anchor.
- **R5 — verify (extension languages).** For each file the run touched: the server's error diagnostics after the run ⊆
  those before (by message + range mapped through the edit); the grammar parses it with no `ERROR`/`MISSING` node.
  Rust files keep Rust's verify.

## Boundaries

- No new Rust assist names; `assisted_edit` stays for existing kinds.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**; implementing one here
collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `extension-language-server` (K22) — **real edge** | `ServerBackend` (`over`, `without_command`), `ServerChoice`, `ExtensionSettings`, `fake_lsp` launched from `settings.json` | every `code_action_acceptance` test starts the fake server from settings and resolves through `ExtensionSupport::backend` → `ServerBackend` | change their signatures. **Green** adds `ApplyCodeAction` to `ServerBackend::supports` and an arm in its `resolve` calling `code_action::resolve` over an `LspClientBridge` of its live client (K22's doc says code actions are K23's), and — open question below — a way to hand it the entry's `code_action_kinds` |
| `extension-outline` (K20) | `load_installed`, `ExtensionSupport` (`claims`, `launch`), `GrammarStore::parse`, `OutlineBackend` | the acceptance tests load the support through `load_installed`; `ExtensionVerify::claims` delegates to `claims`; `ExtensionSupport::parse` (K23's) parses through K20's store | change K20's symbols; it adds one method, `ExtensionSupport::parse`, in its own `impl` block in `support.rs` |
| `extension-install` (K19) | `LanguageServerEntry::code_action_kinds` | green passes it as `only` to `code_action::resolve` | — |
| `language-support` (K11) | `LanguageSupport`, `SupportRegistry`, `LiveServer` | `ExtensionVerify` reads `LanguageSupport::claims`; no new trait method | add a method to `LanguageSupport` — the verify seam is its own trait, `SourceVerify`, carried in `Options::verifies` |
| `backend-fn-sizes` (K21) | the 60-line gate over `backends/` | `backends/code_action.rs` stays under it; the `resolve_opening` arm adds 6 lines to a function already past it (167, K21's to split) | — |

**Sequencing:** the eight `code_action_acceptance` tests reach K20's `load_installed` (`todo!`, `support.rs:48`) first,
then K22's `ServerBackend`, before any K23 body — they go red-for-this-node only once K20 and K22 are green. The tests
attributable to this node alone today: `backends::code_action::tests` (double server), `code_action_fields::tests`,
`verify::tests` (zed-extensions), `commands::tests` and `apply_edit_capture` (tddy-lsp), `verify_by_support`, and the
live `apply_code_action` (reaches `RustBackend::apply_code_action`'s `todo!`).

## Draft PR contract

- Surface: the symbols above, `todo!()` bodies (`TODO(reshape-extension-code-actions)`). Real: the plan field and
  `ActionSelector`'s serde shape, `RefactorKind::ApplyCodeAction`, `SUPPORTED`'s 29th entry and the `resolve_opening`
  arm, `CodeActionRefusal`'s `Display`, `Options::verifies` (default empty), `verify_sources`' empty-verifies early
  return, `ExtensionVerify::{new, claims}`, both `fake_lsp` modes. `code_action_fields::rules` passes every line
  without `action` that is not an `apply_code_action`, so every existing plan parses as before. `apply_edit_answer`
  and the `ApplyEditCapture` are unwired (dispatch still answers `workspace/applyEdit` `null`).
- Acceptance tests (`tddy-zed-extensions/tests/code_action_acceptance.rs`, fake LSP `--code-actions` + `--diagnoses
  BROKEN`): `an_action_carrying_an_edit_is_applied_by_title`, `a_command_action_is_applied_from_the_servers_apply_edit`,
  `two_kind_matches_are_refused_with_both_titles`, `no_match_lists_what_the_server_offered`,
  `a_command_that_sends_no_edit_is_refused`, `verify_fails_on_a_new_error_diagnostic`,
  `verify_fails_on_a_parse_error_node`, `verify_holds_for_a_clean_action`.
  `tddy-code-restructuring/tests/apply_code_action.rs` (live rust-analyzer, 1 test, in the `rust-analyzer`
  nextest group and `rust-e2e.filterset`): `extract_function_by_title_on_a_rust_file`.
  `tddy-lsp/tests/apply_edit_capture.rs`: `an_apply_edit_during_execute_command_reaches_the_caller`,
  `a_command_that_sends_no_edit_reaches_the_caller_with_none`.
  `tddy-code-restructuring/tests/verify_by_support.rs`: `a_changed_extension_file_is_handed_to_its_languages_verify`
  (and `with_no_verify_registered_a_changed_extension_file_has_no_findings`, green on purpose: today's behaviour).
- Unit tests: `plan::codec::code_action_fields::tests` (7 red + `a_line_without_an_action_writes_no_action_field`,
  green on purpose: old plans unchanged); `backends::code_action::tests` (9 red against a server double +
  `a_refusal_lists_what_was_offered_by_title_and_kind`, green on purpose: `Display` is real);
  `tddy-lsp` `client::commands::tests` (2); `tddy-zed-extensions` `verify::tests` (3, diagnostic mapping).

## Green wave

**Wave:** 4 of 4 (unchanged by the contract's tests — no new edge).
**Greenable independently:** no — the acceptance tests need `extension-outline` (K20) and
`extension-language-server` (K22) green; the unit tests, `apply_edit_capture`, `verify_by_support` and the live Rust
test need nothing beyond this node.
**Concurrent with:** `extension-wasm-command` (K24).
**Blocks:** none.

## Decisions (green, 2026-10-23)

The contract's open questions, resolved conservatively:

- **`only` for an extension server.** `ServerBackend::with_code_action_kinds(Vec<String>)` (a field, empty
  asks for every kind); `ExtensionSupport::with_code_action_kinds` carries the manifest entry's
  `code_action_kinds` from `load_installed` (`server::served` now returns them beside the `ServerChoice`, so
  `ServerChoice::Configured` is unchanged) and `LanguageSupport::backend` hands them to the `ServerBackend`.
- **Verify with no server.** `ExtensionVerify::new(support, None)` runs the parse check alone. With a server,
  diagnostics are read by a **pull** (`textDocument/diagnostic`, new `LspClient::pull_diagnostics`) after the
  before / after text is announced, never from the publish cache, which says nothing about the text last sent.
  A server that advertises no `diagnosticProvider` is refused (`ServerDefect`), not silently verified by parse
  alone.
- **Where `verify_sources`' findings surface.** **Resolved 2026-10-23 (developer decision: widen the scope to
  wire it).** `Outcome::VerifiedWithFindings(Comparison, Vec<SourceFinding>)` (only when a verify found
  something; otherwise the unchanged `Verified`); `console::source_findings` renders each as
  `finding: <path>:<line>:<col>: new error diagnostic: <message>` / `…: parse error: ERROR node`. **Findings make
  verify not hold**: the cold command exits non-zero (`console::source_findings_refusal`) and the daemon answers
  `holds: false` with `source_findings` (rendered lines, `repeated string`, field 9; a new field, no new RPC).
  The cold CLI (`tddy-tools-cli`'s `installed_languages`, one `load_installed`, `ExtensionVerify::for_languages`)
  starts **no language server for a verify**, so it judges by the grammar's parse alone — new error diagnostics
  surface through the warm index daemon, which verifies over each touched language's warm server
  (`WorkspaceIndex::with_verifiable`, `runner::changed_sources` picks the languages). Not wired: `tddy-daemon`'s
  hosted code-index service does not call `with_verifiable`, so it still verifies Rust only.
- **Several command edits.** Kept as separate `FileEdit::Change`s in arrival order.
- **`workspace/applyEdit` outside a command** keeps the `null` answer.
- **A Rust backend that spawned its own server** (no shared client) refuses `apply_code_action`: the
  self-spawned transport cannot carry `workspace/applyEdit`.

Sanctioned additions to earlier nodes' files (no signature changed): `ServerBackend::supports` /
`resolve` arm for `ApplyCodeAction` and the `code_action_kinds` field; `server.rs`'s `file_uri`,
`relative_path`, `text_edit_of` made `pub(super)` for `code_action.rs`; `name_position` now reads
`anchor_range`; `tddy-zed-extensions` gains `tokio` (`rt`) as a dependency, already a dev-dependency and a
dependency of `tddy-lsp`.

## Documentation deltas (applied at wrap)

- `rust-code-restructuring.md`: **`apply_code_action`** — a plan line `{"op":"apply_code_action","anchor":<range|item|items>,
  "action":{"title":..}|{"kind":..}}`. Selection is `textDocument/codeAction` over the anchor's range (`title` exact,
  `kind` dotted prefix; none or several matches refuse and list what was offered); resolution is the action's `edit`,
  else `codeAction/resolve`, else its command's `workspace/applyEdit` requests captured during
  `workspace/executeCommand`. The server writes nothing; the run applies the edit like any other. **`## Verify`** —
  `restructure verify` also hands each changed non-Rust file to the `SourceVerify` that claims it
  (`runner::verify_sources`); Rust keeps the statement comparison.
- `zed-extensions.md`: `ExtensionVerify` (new error diagnostics by pull, mapped through the lines the change left
  alone, plus `ERROR` / `MISSING` parse nodes), `code_action_kinds` passed as `context.only`.

## Successor PRs

`feature/reshape/extension-wasm-command` (K24).

## Scope

- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (batched; ticked at `/green` start)
- [x] TDD Red — write failing unit/integration tests
- [x] LSP: executeCommand + applyEdit capture
- [x] Operation (R1–R4), Rust and extension dispatch
- [x] Verify by support (R5)
- [x] Docs: `apply_code_action` and `## Verify` in rust-code-restructuring.md; zed-extensions.md (deltas below, applied to `packages/*/docs` at wrap)
- [x] Widened 2026-10-23 (developer decision): build `Options::verifies` in the cold CLI and the index daemon
- [x] Widened: render `SourceFinding`s in `restructure verify`; carry them in `VerifyResponse.source_findings`
- [x] Widened: acceptance tests — `tddy-tools-cli/tests/restructure_verify_extension_acceptance.rs`,
  `tddy-index-daemon/tests/extension_verify.rs`
- [ ] Code Quality

## Testing plan

Fake-LSP library tests for every rule; one live rust-analyzer test.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._

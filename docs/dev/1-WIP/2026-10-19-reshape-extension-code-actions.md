# Changeset: `apply_code_action` runs any language server's code action by name, and extension runs are verified

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (generic code-action operation; verify for extension languages)
**Stack**: `#reshape` 23/24, branch `feature/reshape/extension-code-actions`, base `feature/reshape/extension-language-server`, green wave 4.

## Initial Discovery

[2026-10-19-reshape-extension-code-actions-initial-discovery.md](2026-10-19-reshape-extension-code-actions-initial-discovery.md)

## Affected Packages

- `packages/tddy-lsp`: `workspace/executeCommand` request; server→client `workspace/applyEdit` delivered to the
  waiting request instead of `null` (`client/dispatch.rs:106`).
- `packages/tddy-code-restructuring`: `RefactorKind::ApplyCodeAction`, `RefactorOp.action`, codec + static check,
  `ServerBackend` and `RustBackend` dispatch, `runner/comparison.rs` verify by support.
- `packages/tddy-zed-extensions`: the extension-language verify (diagnostics + parse).

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-code-actions.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-code-actions.md)

## Summary

A generic `apply_code_action` operation (Rust included, PRD F2 a) and, for extension languages, a verify of "no new
error diagnostics and no parse `ERROR` node" (PRD F1 a).

## Responsibility

| Symbol | Where |
|---|---|
| `RefactorKind::ApplyCodeAction`, `RefactorOp::action: Option<ActionSelector>` (`{title}` \| `{kind}`) | `plan/` |
| `LspClient::execute_command`, applyEdit capture | `tddy-lsp/src/client/` |
| `code_action::resolve(server, op, workspace)` (shared by `ServerBackend` and `RustBackend`) | `tddy-code-restructuring/src/backends/code_action.rs` |
| `ExtensionVerify` (diagnostics + parse) and verify dispatch by support | `tddy-zed-extensions/src/verify.rs`, `runner/comparison.rs` |

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

- `feature/reshape/extension-language-server` (K22) — **real edge**: `ServerBackend`, a launched extension server.

## Draft PR contract

- Surface: the symbols above, `todo!()` bodies (`TODO(reshape-extension-code-actions)`); the plan field real.
- Failing tests: `tddy-zed-extensions/tests/code_action_acceptance.rs` (fake LSP):
  `an_action_carrying_an_edit_is_applied_by_title`, `a_command_action_is_applied_from_the_servers_apply_edit`,
  `two_kind_matches_are_refused_with_both_titles`, `no_match_lists_what_the_server_offered`,
  `a_command_that_sends_no_edit_is_refused`, `verify_fails_on_a_new_error_diagnostic`,
  `verify_fails_on_a_parse_error_node`, `verify_holds_for_a_clean_action`;
  `tddy-code-restructuring/tests/apply_code_action_acceptance.rs` (live rust-analyzer, 1 test):
  `extract_function_by_title_on_a_rust_file`; `tddy-lsp/tests/apply_edit_capture.rs`:
  `an_apply_edit_during_execute_command_reaches_the_caller`; plan codec unit tests for R4.

## Green wave

**Wave:** 4 of 4.
**Greenable independently:** no — needs K22 green.
**Concurrent with:** `extension-wasm-command` (K24).
**Blocks:** none.

## Successor PRs

`feature/reshape/extension-wasm-command` (K24).

## Scope

- [ ] USER REVIEW (ticked at `/green` start)
- [ ] LSP: executeCommand + applyEdit capture
- [ ] Operation (R1–R4), Rust and extension dispatch
- [ ] Verify by support (R5)
- [ ] Docs: `apply_code_action` and `## Verify` in rust-code-restructuring.md; zed-extensions.md
- [ ] Code Quality

## Testing plan

Fake-LSP library tests for every rule; one live rust-analyzer test.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._

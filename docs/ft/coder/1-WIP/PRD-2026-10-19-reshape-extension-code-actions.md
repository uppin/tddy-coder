# `apply_code_action` runs any language server's code action by name, and extension runs are verified - PRD

**Date**: 2026-10-19
**PRD Type**: New feature (generic code-action operation; verify for extension languages)
**Stack**: `#reshape` 23/24 (`feature/reshape/extension-code-actions`, base `feature/reshape/extension-language-server`;
real parent the same — wave 4)

## Affected Features

- [Rust code restructuring](../rust-code-restructuring.md): new operation `apply_code_action` (any language with a
  server, Rust included); `## Verify` for extension languages.
- `docs/ft/coder/zed-extensions.md`: what a plain extension can do.

## Summary

1. **`apply_code_action`** — the way Zed exposes refactors:
   `{"op":"apply_code_action","anchor":{…range…},"action":{"title":"Extract function"}}` or `{"kind":"refactor.extract"}`.
   The engine asks `textDocument/codeAction` over the anchor's range, picks the one action whose title equals `title`
   (or whose kind starts with `kind`; more than one match is refused, listing them), resolves it
   (`codeAction/resolve`) and:
   - takes its `edit` as the run's edit; or
   - runs its `command` with `workspace/executeCommand` and takes the edit the server sends back through
     `workspace/applyEdit` during that call (answered `applied: true` once captured; nothing is written by the server).
2. **Verify for extension languages** (`restructure verify` today is Rust-only — `runner/comparison.rs:79`): F1 decides
   the definition; recommended: after a run, the language server reports **no new error diagnostics** for the files the
   run touched, and the grammar parses each of them with **no `ERROR` node**.

## Background

- Rust already runs assists by exact title (`assisted_edit`); this generalises it to any server and kind.
- `workspace/executeCommand` is not used anywhere and a server-initiated `workspace/applyEdit` is answered `null`
  (discovery §7) — many servers (e.g. ones that apply refactors via commands) need both.

## Proposed Changes

### What's Changing

- `RefactorKind::ApplyCodeAction` with `action: { title } | { kind }` on `RefactorOp` (plan codec, static check of the
  field shape).
- `tddy-lsp`: an `executeCommand` request; server→client `workspace/applyEdit` delivered to the waiting request's
  owner instead of `null`.
- `ServerBackend` (K22) implements it; `RustBackend` gains it too (one dispatch arm through the same generic code).
- Verify for a non-Rust support per F1; `restructure verify` picks the definition by the files' supports.

### What's Staying the Same

- Rust's existing assist-backed operations and the Rust verify passes.

## Acceptance Criteria

- [ ] Against the fake LSP, a title match whose action carries an `edit` is applied; one that carries a `command` is
      applied from the `workspace/applyEdit` the server sends during `executeCommand`.
- [ ] Two actions matching `kind` are refused with both titles listed; none matching lists what the server offered.
- [ ] `apply_code_action` works on a Rust file against rust-analyzer (one live test).
- [ ] Verify fails when a run leaves a new error diagnostic or an `ERROR` parse node in an extension-language file.

## Open questions

- **F1 — what "verify" means for a language without tddy-specific passes.** (a) No new error diagnostics + no parse
  `ERROR` node (recommended: semantic and language-agnostic); (b) a tree-sitter token multiset compare modulo declared
  renames (catches lost text, like Rust's verify, but rejects every legitimate code action); (c) both.
- **F2 — Rust gets `apply_code_action`.** (a) Yes (recommended; one generic path); (b) extension languages only.

## References

- Discovery §7 (LSP coverage), §6 (verify), §8 (`RefactorKind`).

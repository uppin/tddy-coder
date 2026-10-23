# 2026-10-23 — `apply_code_action` gaps: a self-spawned Rust backend, `ContentModified`, edit-plus-command actions

**Category:** Recorded debt from `reshape-extension-code-actions` (`#reshape` 23/24, PR #682)
**Source:** [2026-10-23-reshape-extension-code-actions.md](../changesets/2026-10-23-reshape-extension-code-actions.md) § Decisions
**Owner:** the `#reshape` sweep node

## 1. A `RustBackend` that spawned its own server refuses the operation

`RustBackend::apply_code_action` runs on the shared `LspClientBridge` (the warm daemon's client, or the
one a host hands in). A backend that started its own rust-analyzer has no bridge, and the operation is
refused naming that. The self-spawned transport cannot carry the server's `workspace/applyEdit` requests
back to the waiting command. The cold CLI's Rust runs go through the self-spawned path, so on the cold
path `apply_code_action` works for extension languages only.

**Wanted:** the self-spawned transport answers server requests through the same dispatch as `LspClient`,
or the Rust backend always runs on an `LspClient`.

## 2. No `ContentModified` retry on the code-action requests

`textDocument/codeAction`, `codeAction/resolve` and `workspace/executeCommand` are each asked once through
the `CodeActionServer` bridge. rust-analyzer answers `ContentModified` (-32801) while it re-indexes after a
change. The bridge maps that answer to `RestructureError::ServerCatchingUp` (`lsp_bridge.rs`
`map_lsp_error`), but `code_action::resolve` has no loop that retries it, so an action asked right after an
edit fails the run. The Rust backend's own requests retry the same answer up to `CONTENT_MODIFIED_RETRIES`
(`backends/rust.rs`).

**Wanted:** `code_action::resolve` retries `ServerCatchingUp` with the same bound the Rust backend's
requests use.

## 3. An action with both an `edit` and a `command` runs only its edit

R2 takes an action's `edit` as its resolution and runs its `command` only when the action has no edit.
The LSP specification says a client applies the edit and **then** runs the command.
`an_action_carrying_an_edit_and_a_command_resolves_to_its_edit_without_running_the_command`
(`backends/code_action/tests.rs`) pins today's R2. A server whose command finishes the refactor (for
example, by renaming what the edit introduced) gets half of its action applied.

**Wanted:** run the command after the edit is staged and capture its `workspace/applyEdit` requests as R2
already does for a command-only action. Merge both into the run's edit. Then update R2 and that test.

# 2026-10-23 — any language server's code action runs by name, and a run over an extension language's files is verified

`#reshape` 23/24 ([#682](https://github.com/uppin/tddy-coder/pull/682)).

- **`apply_code_action`.** A plan line names a code action by its exact `title` or by a `kind` (a dotted prefix)
  over an anchor. The run asks the file's server what it offers, takes the one action that matches, and applies its
  edit: the action's own, the resolved one, or the edits its command sends back. The server writes nothing. Works for
  Rust (rust-analyzer's actions, on the warm daemon) and for every installed extension language with a server,
  asked for the kinds its manifest entry declares.
- **Refusals that say what was offered.** No match lists every action the server offered; several matches list
  them; a command that sends no edit is refused naming the action.
- **Verify beyond Rust.** `restructure verify` hands every changed non-Rust file to its installed language's verify:
  an error diagnostic the change introduced, or a parse error in the grammar, is shown as a `finding:` line and fails
  the verify, cold and through the index daemon (`VerifyResponse.source_findings`). The cold command checks the
  parse alone.

Feature docs: [restructure § `apply_code_action`](../rust-code-restructuring.md#apply_code_action),
[restructure § Files of other languages](../rust-code-restructuring.md#files-of-other-languages-r5),
[Zed extensions § Language servers](../zed-extensions.md#language-servers).

# Commands and pulled diagnostics (tddy-lsp)

Two client operations that a code action and a verify are built on. They are the only places this crate
answers a server's own request with something other than `null`, and the only place it asks a server for
diagnostics instead of reading what it published.

## `LspClient::execute_command`

`execute_command(command, arguments)` sends `workspace/executeCommand` and answers
`ExecutedCommand { result, applied_edits }`:

- **`applied_edits`**: the `edit` of every `workspace/applyEdit` the server sends **while the command
  runs**, in arrival order. Each is answered `{ "applied": true }`. Nothing is written to disk: the caller
  applies the edits through its own path.
- **`result`**: the server's own answer to the command. No caller in this repository reads it today. It
  stays because a general client hands back what the server answered.
- **One command at a time per client.** The call holds a per-client gate (`command_gate`) for its whole
  length, so a second call on the same shared client waits. Without the gate, two concurrent commands would
  each see the other's edits.
- **An `applyEdit` with no `edit`** during a command is answered
  `{ "applied": false, "failureReason": … }` and is not captured.
- **An `applyEdit` outside any command** keeps the `null` answer.

The capture's lock recovers from poisoning: every write under it is one whole assignment or one `push`, so
a panicking holder cannot leave it half-updated, and closing the capture must not panic.

## `LspClient::pull_diagnostics`

`pull_diagnostics(uri)` sends `textDocument/diagnostic` and returns the items of the full report. It never
reads the publish cache. That cache holds whatever the server last chose to publish, and says nothing about
the text it was last sent. A server reads its messages in order, so the answer covers every change
announced before the call. A caller checks the server's `diagnosticProvider` capability first:
`tddy-zed-extensions`' `ExtensionVerify` refuses a server that has none.

## The fake server

`tests/bin/fake_lsp.rs` exercises both:

- `--code-actions <file>` offers the file's actions. Its commands send the edits listed under
  `command_edits`. `tddy/applyEditAnswers` replays what the client answered (a whole error response when
  there was no `result`), and `tddy/codeActionRequests` replays the params of each
  `textDocument/codeAction` request, in order.
- `--diagnoses <marker>` reports every occurrence of the marker as an error, with ranges in UTF-16 code
  units from the line start.

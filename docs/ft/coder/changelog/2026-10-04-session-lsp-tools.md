# 2026-10-04 — A session's `Lsp*` tools are answered by the warm index

`#live-plan` 11/15, PR [#570](https://github.com/uppin/tddy-coder/pull/570). Feature:
[Warm code-intelligence daemon](../warm-code-intelligence-daemon.md); also
[Reusable LSP](../reusable-lsp.md).

When the daemon manages an index (`index_daemon:`), an agent's `LspDefinition`, `LspReferences`,
`LspHover`, `LspSymbols` and `LspDiagnostics` calls are answered by it, rooted at the session's own
worktree, so a session and the web's code pane ask the same index and no second rust-analyzer is started
for a Rust worktree.

- The index gains two RPCs for the two tools that had none: `Symbols` (a file's symbols, or the root's
  symbols matching a query) and `Diagnostics` (what the server reports wrong with one file).
- The worktree is bound on the host, from the session; a query naming a file outside it — another
  session's worktree, a path climbing out with `..` — is refused before the index is asked.
- Tool names, argument schemas and results are unchanged, and without an `index_daemon:` section the
  tools behave exactly as before.
- `ReadLints` is refused through the index, because diagnostics are answered per file. Backlog entry
  `2026-10-04-read-lints-is-refused-through-the-warm-index`.

Acceptance criteria reviewed and signed off by the developer: each was met by a passing test —
`LspDefinition` through the index for the session worktree, a path outside it refused on the host,
symbols and diagnostics answered by the index, the existing executor answering without an index, and
the advertised tool set unchanged.

Cross-package entry:
[2026-10-04-session-lsp-tools.md](../../../dev/changesets/2026-10-04-session-lsp-tools.md).

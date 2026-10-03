# 2026-10-03 — Go to definition, hover and references in the session code pane

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574).

Rust files in the [code pane](../session-code-pane.md#code-navigation) are navigable. Ctrl/cmd-click an
identifier to open its definition in the same pane, scrolled to the line; rest the pointer on one to see
its type and docs; use the card's **References** action to list every reference and open any of them.
Answers come from the warm rust-analyzer index through the session host's daemon, which authorises
requests like file reads. Other file types are unchanged. A daemon with no `index_daemon:` configuration
section tells the operator so instead of falling back to another language server.

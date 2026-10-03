# 2026-10-03 — `Definition`, `References` and `Hover` RPCs

**Type:** Feature

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574). Cross-package entry:
[2026-10-03-code-navigation.md](../../../../docs/dev/changesets/2026-10-03-code-navigation.md).

`code_index.CodeIndexService` gains three unary RPCs, implemented in `src/navigation.rs` as one
`LspClient` query each on `WorkspaceIndex::client_for(root)`. Requests are `{workspace_root, file,
position}` in one-based line / one-based byte-column coordinates; the zero-based LSP conversion is the
service's (the client negotiates `utf-8` position encoding, so no UTF-16 arithmetic). Locations return
relative to the root, or absolute with `outside_root` set. Only `.rs` files are served; empty,
absolute or `..` paths, a missing position and zero coordinates are `InvalidArgument`, an unreadable
file `NotFound`. The file's current text is synced to the server before each query, and outgoing
`file://` URIs are percent-escaped (round-trip unit test).

Tests: `code_index_service_acceptance.rs` § Navigation (definition, references, hover) over `fake_lsp`
started with the new `--answers-in-its-workspace` flag (`packages/tddy-lsp/tests/bin/fake_lsp.rs`).

Code issues: both open records (`complexity-warm-narrate-until-loaded`,
`poisoned-warm-latch-on-interrupted-index`) name code this change did not touch; left unchanged.
Three tests in this package fail with `navigation.rs` reverted and were left alone
(`warming_a_root_forwards…`, `exits_non_zero_when_the_tree_no_longer_holds_against_the_ref`,
`a_test_binary_move_in_plan_a_moves_plan_bs_file_hint`).

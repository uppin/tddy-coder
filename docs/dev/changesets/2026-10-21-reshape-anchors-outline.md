# 2026-10-21 — #reshape 16/24 `anchors-outline`

**Type:** Fix · **Packages:** `tddy-lsp`, `tddy-index-daemon`, `tddy-code-restructuring`

A language server that never started is refused naming the program and the cause (`tddy-lsp`), and the index
daemon reports it unavailable. Static `check` checks an item anchor's crate/module prefix. The v2 header's
`modified` hint and the dead `anchor_for` path are removed. Closes the code issues
`broken-restructure-anchors-empty-outline` and `dead-code-plan-filehint-modified` and the todo
`2026-10-02-static-check-cannot-verify-item-anchors`. Leaves three todo entries (spawn record for read-only
cold runs, the `<Trait for Type>` hint, launch pinning).

Details: [tddy-code-restructuring](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-21-anchors-outline.md),
[tddy-lsp](../../../packages/tddy-lsp/docs/changesets/2026-10-21-server-not-started.md).

# 2026-10-03 — `CodeNavigationService`: the first caller of `IndexDaemonRegistry::connect`

**Type:** Feature

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574). Cross-package entry:
[2026-10-03-code-navigation.md](../../../../docs/dev/changesets/2026-10-03-code-navigation.md).

`src/code_navigation.rs` serves `code_navigation.CodeNavigationService`: each request is authorised
by `WorktreeServiceImpl::resolve_listed_worktree`, its `rel_path` shape-checked, and forwarded to
`code_index.CodeIndexService` over the channel `IndexDaemonRegistry::connect` dials (starting the
index daemon on first use). No `index_daemon:` section is `FailedPrecondition`, a failed start or dial
is `Unavailable`, and nothing falls back to `tddy_lsp_executor`. `runtime.rs` registers the entry.
`tddy-index-daemon` is now a `[dependencies]` entry. See
[code-navigation-service.md](../code-navigation-service.md).

Tests: `tests/code_navigation_acceptance.rs` (forwarding, unlisted worktree, missing section, lazy
start) with the production registry over a stand-in index daemon, and four unit tests in the module;
`tests/test_placement.rs` lists the suite.

Code issues: `oversized-file-runtime` grew 1,620 → 1,631 lines and `complexity-runtime-build` 880 → 891
(`build` function); both records carry a 2026-10-03 row, the split is deferred with the developer's
consent. `complexity-runtime-build` and `oversized-file-runtime` remain open and unclaimed.

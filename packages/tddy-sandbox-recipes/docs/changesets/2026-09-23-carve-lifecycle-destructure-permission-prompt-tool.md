# 2026-09-23 — `PERMISSION_PROMPT_TOOL` is exported

**Type:** Refactor

`#carve` 14/15, PR [#524](https://github.com/uppin/tddy-coder/pull/524), DRY #13 (`1fd2b7bc`).
Cross-package entry: [2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

`claude_cli::PERMISSION_PROMPT_TOOL` is public and re-exported from the crate root, so
`tddy-session-lifecycle`'s split session names it instead of defining its own copy.

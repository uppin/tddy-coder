# 2026-10-22 — build_auth_entries_admitting to 58 lines and 5 parameters

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `build_auth_entries_admitting` **111 lines, 6 parameters → 58, 5**: three `extract_method`s, then by
  hand with the developer's consent a `let … else` for the no-`github:` return, `provider_auth_entry`,
  and the web host and port folded into one `(host, port)` value (its callers updated). The four helpers
  it calls moved to `auth/entry_parts.rs` (105) by `move_item`.
- Record closed and deleted: `complexity-auth-build-auth-entries-admitting.md` (111 lines / 6 params →
  58 / 5). `oversized-file-auth.md` narrowed, 616 → 569: the file was over budget before the `#satellite`
  stack, which only shrank it.

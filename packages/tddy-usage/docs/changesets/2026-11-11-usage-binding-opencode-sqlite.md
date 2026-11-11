# 2026-11-11 — tddy-usage: the `sqlite` format

**Type:** Feature

`UsageSource::Sqlite(SqliteSource)` with `RootMatch`, evaluated by `sqlite.rs` over a read-only `rusqlite` connection (`rusqlite 0.32`, bundled, the version `tddy-semantic-index` uses). `#usage-binding 5/5`, PR [#894](https://github.com/uppin/tddy-coder/pull/894). See [usage-bindings.md](../usage-bindings.md).

- The connection is opened with open flags (`SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX`) and a 200 ms busy timeout, not the planned `mode=ro` URI; the effect is the same.
- `validate_usage_section` calls `check_sqlite` beside `check_source`; `tree.rs` exposes `render_main`, `MAX_DEPTH` and `record_of` to the new module; `CounterExpr::paths` lists the columns an expression reads.
- The recursive walk's state is a `Walk` struct rather than eight parameters.
- No file passes 500 production lines (`sqlite.rs` ~315, `section.rs` ~340, `tree.rs` ~367).

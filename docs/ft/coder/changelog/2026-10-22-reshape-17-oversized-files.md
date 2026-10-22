# 2026-10-22 — production lines leave out every test-only item; `restructure lines` reports them

`#reshape` 17/24. A Rust file's production lines are now every line except those of each `#[cfg(test)]` or
`#[cfg(all(test, …))]` item, wherever it sits — the old count stopped at the first `#[cfg(test)] mod`, so a file
that declared an extracted test module near its top (`runner/tidy.rs`: 36 measured, 542 real) slipped under the
500-line budget. `check --budget` uses the new count, and the new `tddy-tools restructure lines <file>...` prints it
per file, in process, with no server. The engine's own oversized files are now measured correctly; splitting them is
deferred to the `#reshape` sweep node. See [Rust code restructuring § Commands](../rust-code-restructuring.md).

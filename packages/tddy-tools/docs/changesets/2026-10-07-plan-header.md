# 2026-10-07 — the CLI pins `snapshot` writing a missing header

**Type:** Tests

No source change: `restructure snapshot`'s routing was already correct. Three tests in
`tests/restructure_cli_acceptance.rs` pin it — the CLI writes the header for a plan of bare operations
and leaves the operation lines byte-identical; it does not dial a named daemon (a `TDDY_INDEX_SOCKET`
pointing at a socket nothing listens on still exits 0); and `restructure check` of a headerless plan
names `restructure snapshot` as the remedy.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-plan-header.md).

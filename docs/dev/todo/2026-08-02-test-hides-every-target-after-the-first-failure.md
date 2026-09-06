# 2026-08-02 — `./test` hides every target after the first failure

**Category:** Future enhancement

`./test` runs bare `cargo test`, which **aborts remaining test targets on the first failing one**
unless `--no-fail-fast` is passed. On any host where an earlier target fails, every later package
silently never runs — and the summary still looks like a complete run, because the passed-count is
simply the truncated total.

This bit for real: the pre-existing `action_sandbox_acceptance` failure (see below) meant
`packages/tddy-supervisor` — alphabetically later — was never executed by `./test`, while the printed
total was indistinguishable from a full pass. `./test --no-fail-fast` works today (the script forwards
`"$@"`), so the fix is to make that the default, or to say so loudly in the usage comment. Given
`./test`'s stated purpose is agent-readable verification evidence, a summary that can silently omit
whole packages is the wrong default.

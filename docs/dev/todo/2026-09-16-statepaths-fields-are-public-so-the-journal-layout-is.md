# 2026-09-16 — `StatePaths`' fields are public, so `.restructure/`'s layout is public API

**Category:** Deferred refactor
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M3

`StatePaths`, `open_run`, `restore_ledger` and `commit_operation`
(`packages/tddy-code-restructuring/src/runner.rs`) were promoted to `pub` so a host can drive the
apply loop without re-deriving the `.restructure/` paths or re-implementing the write-ahead commit
sequence. `tddy-index-daemon`'s `src/apply.rs` is that host, and it is why the promotion happened.

The cost is that `<root>/.restructure/journal.jsonl` and `ledger.json` are now part of this crate's
public surface. The change that re-keys the journal to a plan identity — recorded in
[2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](./2026-09-09-restructure-defects-from-the-first-cross-crate-move.md),
whose first bullet is "the journal is repo-scoped, not plan-scoped" — therefore becomes a breaking
change for anything reading those fields.

## The narrower alternative

Keep the fields private and expose only `StatePaths::under` plus `open_run` / `restore_ledger` /
`commit_operation`. That serves the apply loop, which is the actual requirement, and leaves a host
unable to read the journal for a status-style query.

Whether that matters depends on one thing: `tddy-index-daemon`'s `serve_plan_status` currently
delegates to `runner::status`, which returns `PlanProgress` — so it needs no path at all. If nothing
else grows a need, the fields can go private again before anything depends on them. Check that before
the journal re-keying, not after.

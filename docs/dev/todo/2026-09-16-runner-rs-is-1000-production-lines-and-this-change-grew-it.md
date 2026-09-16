# 2026-09-16 — `runner.rs` is a thousand production lines, and the daemon work grew it

**Category:** Deferred refactor
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, `/validate-changes`

`packages/tddy-code-restructuring/src/runner.rs` is **1,403 lines, 1,018 of them production**
(tests from `:1019`). It was already 1,261 at `master`, so it was over the ~500-line guideline before
this change and is further over after it: net `+142`.

What the daemon work did to it was, on balance, a reduction in responsibility — 23 `println!`
removed, the five entry points returning values instead of printing, the console moved to
`restructure_cli.rs` — and the file still grew, because the `Outcome` enum, `Finding`,
`PlanProgress`, `RunSummary`, the three caller-owned sink fields on `Options` and a workspace root on
every signature all landed in it.

## Why it was not split here

`CLAUDE.md`: *"never absorb a large refactor into a feature change, which buries a reviewable diff
under a mechanical one."* `runner.rs` already carries 550 changed lines in this branch; a split on
top would make the feature unreviewable. The same reasoning
[`2026-09-09-coverage-rs-module-hygiene.md`](./2026-09-09-coverage-rs-module-hygiene.md) gives for
leaving `coverage.rs` alone applies verbatim.

## The seams, which this change made visible

The file is four responsibilities that barely reference each other, and the sink and root parameters
now make the boundaries explicit:

- **The option surface** — `Options`, `Command`, `parse_options` / `absorb`, `USAGE`, and their
  arg-parsing tests. `restructure_cli.rs` already gave up its clap half to a new
  `restructure_args.rs` for exactly this reason; this is the same split one layer down.
- **The result vocabulary** — `Outcome`, `Finding`, `PlanProgress`, `RunSummary`. Pure data with no
  behaviour, imported by three crates.
- **The run state** — `StatePaths`, `open_run`, `restore_ledger`, `commit_operation`. Now `pub`, so
  this is already a published cluster with one job: the `.restructure/` write-ahead sequence.
- **The five entry points** — `apply`, `check`, `status`, `anchors`, `verify` plus `dispatch`, which
  is what would be left.

Worth doing as its own PR, after the daemon change lands, while the four groups are still this
distinct.

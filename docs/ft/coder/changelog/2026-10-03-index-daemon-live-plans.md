# 2026-10-03 — Every loaded plan stays current, and the restructure tooling stops leaving work behind

`#live-plan` 7/15, PR [#539](https://github.com/uppin/tddy-coder/pull/539). Features:
[Rust code restructuring](../rust-code-restructuring.md),
[Warm code-intelligence daemon](../warm-code-intelligence-daemon.md).

**Live plans.** A multi-plan carve no longer needs re-anchoring between plans or after a rebase.

- An operation applied from one loaded plan is folded into every other loaded plan of the root: ranges
  move with the lines inserted above them, `file` hints follow a renamed or moved file, and an edit
  inside another plan's anchored range marks that operation stale (`edited by <plan>#<op>`) rather than
  translating it.
- Files changed underneath the daemon (a checkout, a rebase, a hand edit) are re-resolved for every
  loaded plan's item anchors; a changed item is `item changed`, a vanished one `item not found in <file>`.
- Stale operations are listed by `ListPlans` and `PlanStatus`, reported by `check`, and refused by
  `apply` before any write, for the operations the run would reach (`--stop-after` is honoured; a dry run
  is not refused).
- `restructure snapshot` of an item-anchored plan re-resolves its anchors and reports what it cannot.
- Staleness is derived and held in memory for the life of a load; plans nobody loaded are never touched.

**Restructure tooling.**

- `apply` ends with a tidy: imports rustc reports unused are removed, an import only the parent's tests
  use is gated `#[cfg(test)]`, and every file written is `rustfmt`ed. A tidy that cannot repair a round
  undoes it and fails the run.
- `check --budget` counts production lines.
- `anchors --items` takes module-qualified names and `<Type>` / `<Type>#N` for an inherent `impl`.
- `verify` compares logical statements and prints one summary line of what an `extract_module` always
  causes, so its exit status is a signal; a real loss prints `verify: tokens lost: …; tokens gained: …`.
- `extract_module` carries a parent's `use`-bound name that shadows a prelude name, keeps a widened
  item's type widened, rebases relative visibility one level deeper, and re-roots inline `super::` /
  `self::` paths.
- rust-analyzer progress is one line per token every two seconds on the console, and every phase on the
  warm stream.
- `./run-index-daemon` runs a prebuilt daemon when `TDDY_INDEX_DAEMON_BIN` names one.

Known limitations are in the feature documents.

# 2026-10-02 — Extractions refuse or repair what rust-analyzer gets wrong, and never hang

`#live-plan` 5/7, PR [#542](https://github.com/uppin/tddy-coder/pull/542). Feature:
[Rust code restructuring](../rust-code-restructuring.md).

Four extraction defects, found while running real plans, no longer cost a hand fix or a restart of
the warm daemon.

- **`extract_variable` no longer waits for ever** on a range that opens with `&`. It asks the server
  at the first position that can answer, and a probe that stays silent for 30 seconds once the index
  is ready ends as `rust-analyzer's answer was unusable:`, so the warm workspace answers the next
  request.
- **A borrowed place is bound as the borrow.** Extracting `self.v` from `&self.v` yields
  `let x = &self.v;` instead of a move out of `&self`. When the borrow is on another line the
  selection is refused, naming it. The refusal does not look at the type, so a `Copy` place under a
  borrow is refused too; select the borrow including its `&`.
- **An early `return` before a unit tail is refused.** An `extract_method` range holding a `return`
  and running to the end of a function that returns `()` is refused by `check`, `check --deep` and
  `apply` before any edit, advising to start the range after the early exit.
- **A function-local `use` goes with the code that needs it.** `extract_method` writes the origin's
  function-local `use` items that the range names into the new function. The origin keeps its own,
  which can be an unused-import warning; `check` reports the carried items on its progress line.

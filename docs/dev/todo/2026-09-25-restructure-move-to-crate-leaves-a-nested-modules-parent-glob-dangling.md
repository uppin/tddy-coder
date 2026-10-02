# 2026-09-25 — moving a nested module to a crate does not count the items its parent's re-export makes visible, nor does `check --deep` say so

**Category:** Future enhancement (engine gap; the move itself is correct)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`22787218:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/05a-attachment-progress-to-session-files.jsonl`,
op 0 (`move_module_to_crate`: `connection_service::attachment_progress` → `tddy-session-files`,
`reexport: glob`)


## What remains

The parent's `use <module>::*;` and `use <module>::{…};` are rewritten to the destination, and the
origin compiles (`#live-plan` 4/7, [#541](https://github.com/uppin/tddy-coder/pull/541)). Two halves of
the original ask are not done:

- **Count every item the re-export makes visible as reached from outside**, so the widening question
  is asked. A glob names no items, and the survey that would list them belongs to `move-paths`.
- **`check --deep` should say so:** "0 caller(s)" beside "5 item(s) reached from outside" is the
  signature of a re-exported module. That is the `check-parity` node's surface
  ([#543](https://github.com/uppin/tddy-coder/pull/543)).

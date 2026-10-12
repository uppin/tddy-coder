# 2026-10-09 — `inline_method`'s output is pinned by no test, and its expected shape does not compile in common cases

**Category:** Test gap / limitation (an advertised operation whose output was never checked live)
**Source:** #reshape 18/19 (`feature/reshape/backend-session`), choosing the route from method to function (decision F1).

## What was found

- `inline_method` (`RefactorKind::InlineMethod`, rust-analyzer's "inline into all callers", resolved through
  `multi_file_assist`) is in `SUPPORTED`. No test under `packages/tddy-code-restructuring/tests/` applies it.
  `plan-schema.md` documents one limit: a call inside a macro is not rewritten, and the definition stays.
- rust-analyzer's `inline_call` handler substitutes an argument directly only when it is a local name, a literal or a
  closure. **It binds every other argument with `let`**, so `self.settled_outline(&uri)` inlines as a block
  `{ let uri = &uri; … }`.
- A method whose body starts `let x = self;` (the shape `read_fields_through`'s self mode writes) is copied into every
  caller. There it **moves** `self` (`&mut` is not reborrowed by an unannotated `let`), so the caller's next `self.` is
  E0382. A caller whose receiver has the same name gets `let x = x;` (clippy `redundant_locals`).
- The definition is removed, but an `impl` block it empties stays.
- A private item of the callee's module that the inlined body names is not widened. At a caller in another module that is
  E0603, or E0425 when the path cannot be qualified.

These are expected from the handler's design and were **not reproduced live** here. #reshape 18 chose another route and
did not need to.

## What closing it takes

A thin live binary that inlines a method with name, expression and `self` arguments, and a caller in another module.
Either it pins the output and the docs state the limits, or the engine refuses the shapes it cannot inline cleanly:
an argument that would be `let`-bound, a body that rebinds `self`, a private item named across modules.

## Why deferred

No current plan uses `inline_method`, and #reshape 18's `detach_method` covers its one planned use. A live reproduction
costs a rust-analyzer suite of its own.
